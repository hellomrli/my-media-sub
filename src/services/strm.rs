use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::clients::NormalizedItem;
use crate::error::{AppError, Result};
use crate::models::{Settings, Subscription};
use crate::utils::write_file_atomic;

#[derive(Debug, Clone)]
pub struct StrmGeneratedFile {
    pub fid: String,
    pub file_name: String,
    pub strm_path: PathBuf,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct StrmGenerationResult {
    pub generated_count: usize,
    pub skipped_count: usize,
    pub output_dir: PathBuf,
    pub files: Vec<StrmGeneratedFile>,
}

pub fn strm_generation_enabled(settings: &Settings, sub: &Subscription) -> bool {
    settings.strm_enabled && sub.strm_enabled
}

/// 异步生成 STRM 文件：把阻塞的文件系统操作放到 `spawn_blocking`，避免阻塞 tokio executor 线程。
pub async fn generate_subscription_strm_files_async(
    settings: &Settings,
    sub: &Subscription,
    target_dir: &str,
    files: &[NormalizedItem],
) -> Result<StrmGenerationResult> {
    let settings = settings.clone();
    let sub = sub.clone();
    let target_dir = target_dir.to_string();
    let files = files.to_vec();

    tokio::task::spawn_blocking(move || {
        generate_subscription_strm_files(&settings, &sub, &target_dir, &files)
    })
    .await
    .map_err(|e| AppError::Internal(format!("STRM 生成任务执行失败: {}", e)))?
}

pub fn generate_subscription_strm_files(
    settings: &Settings,
    sub: &Subscription,
    target_dir: &str,
    files: &[NormalizedItem],
) -> Result<StrmGenerationResult> {
    if !strm_generation_enabled(settings, sub) {
        return Ok(StrmGenerationResult {
            generated_count: 0,
            skipped_count: files.len(),
            output_dir: PathBuf::new(),
            files: Vec::new(),
        });
    }

    let output_dir = subscription_strm_output_dir(settings, target_dir)?;
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| AppError::Internal(format!("创建 STRM 目录失败: {}", e)))?;

    let mut generated = Vec::new();
    let mut skipped_count = 0usize;
    let mut used_names: HashSet<String> = HashSet::new();

    for file in files {
        if !file.file
            || file.fid.trim().is_empty()
            || !crate::services::is_video_name(&file.file_name)
        {
            skipped_count += 1;
            continue;
        }

        let url = httpstrm_url(settings, &file.fid, &file.file_name)?;
        let strm_name = unique_strm_file_name(&file.file_name, &mut used_names);
        let strm_path = output_dir.join(&strm_name);
        write_file_atomic(&strm_path, format!("{url}\n").as_bytes(), 0o644)?;

        generated.push(StrmGeneratedFile {
            fid: file.fid.clone(),
            file_name: file.file_name.clone(),
            strm_path,
            url,
        });
    }

    Ok(StrmGenerationResult {
        generated_count: generated.len(),
        skipped_count,
        output_dir,
        files: generated,
    })
}

fn subscription_strm_output_dir(settings: &Settings, target_dir: &str) -> Result<PathBuf> {
    let root = settings.strm_output_dir.trim();
    if root.is_empty() {
        return Err(AppError::Validation("未配置 STRM 输出目录".to_string()));
    }

    let mut path = PathBuf::from(root);
    for part in normalized_relative_parts(target_dir) {
        path.push(part);
    }
    Ok(path)
}

pub fn httpstrm_url(settings: &Settings, fid: &str, file_name: &str) -> Result<String> {
    let base = settings.strm_public_base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(AppError::Validation(
            "未配置 HTTPStrm 对外访问地址".to_string(),
        ));
    }

    let token = settings.strm_access_token.trim();
    if token.is_empty() {
        return Err(AppError::Validation(
            "未配置 HTTPStrm 访问 Token".to_string(),
        ));
    }

    Ok(format!(
        "{base}/strm/quark/{}/{}?token={}",
        percent_encode_path_segment(fid),
        percent_encode_path_segment(file_name),
        percent_encode_query_value(token)
    ))
}

fn strm_file_name(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(file_name);
    format!("{}.strm", sanitize_path_segment(stem))
}

/// 生成不与已用名冲突的 STRM 文件名。
///
/// 不同扩展名的同名源文件（如 `video.mp4` 与 `video.mkv`）会归一化为同一个
/// `video.strm`，从而相互覆盖。这里在冲突时回退到带原扩展名的名字
/// （`video.mkv.strm`），仍冲突再追加序号，确保每个源文件都有独立的 `.strm`。
fn unique_strm_file_name(file_name: &str, used_names: &mut HashSet<String>) -> String {
    let primary = strm_file_name(file_name);
    if used_names.insert(primary.clone()) {
        return primary;
    }

    // 用完整文件名（含扩展名）作为去重词干。
    let full = sanitize_path_segment(file_name);
    let with_ext = format!("{}.strm", full);
    if used_names.insert(with_ext.clone()) {
        return with_ext;
    }

    let mut counter = 1usize;
    loop {
        let candidate = format!("{}.{}.strm", full, counter);
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        counter += 1;
    }
}

fn normalized_relative_parts(path: &str) -> Vec<String> {
    path.replace('\\', "/")
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .map(sanitize_path_segment)
        .collect()
}

fn sanitize_path_segment(value: &str) -> String {
    let value = value
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if value.is_empty() {
        "未命名".to_string()
    } else {
        value
    }
}

fn percent_encode_query_value(value: &str) -> String {
    percent_encode(value, |byte| {
        matches!(
            byte,
            b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'.'
                | b'~'
        )
    })
}

fn percent_encode_path_segment(value: &str) -> String {
    percent_encode(value, |byte| {
        matches!(
            byte,
            b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'.'
                | b'~'
        )
    })
}

fn percent_encode(value: &str, keep: impl Fn(u8) -> bool) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        if keep(*byte) {
            encoded.push(*byte as char);
        } else {
            encoded.push_str(&format!("%{:02X}", byte));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video_item(fid: &str, name: &str) -> NormalizedItem {
        NormalizedItem {
            fid: fid.to_string(),
            parent_fid: "parent".to_string(),
            file_name: name.to_string(),
            file: true,
            is_dir: false,
            size: 0,
            updated_at: String::new(),
        }
    }

    fn subscription() -> Subscription {
        serde_json::from_value(serde_json::json!({
            "id": "sub",
            "title": "庆余年",
            "url": "https://pan.quark.cn/s/test",
            "created_at": 1,
            "updated_at": 1,
            "last_checked_at": 1,
            "strm_enabled": true
        }))
        .unwrap()
    }

    #[test]
    fn httpstrm_url_encodes_path_and_query() {
        let settings = Settings {
            strm_public_base_url: "https://media.example.com/".to_string(),
            strm_access_token: "token with space".to_string(),
            ..Default::default()
        };

        let url = httpstrm_url(&settings, "fid/1", "庆余年 S01E01.mkv").unwrap();

        assert_eq!(
            url,
            "https://media.example.com/strm/quark/fid%2F1/%E5%BA%86%E4%BD%99%E5%B9%B4%20S01E01.mkv?token=token%20with%20space"
        );
    }

    #[test]
    fn generate_subscription_strm_files_mirrors_target_dir() {
        let root = std::env::temp_dir().join(format!("my-media-sub-strm-{}", uuid::Uuid::new_v4()));
        let settings = Settings {
            strm_enabled: true,
            strm_output_dir: root.to_string_lossy().into_owned(),
            strm_public_base_url: "http://127.0.0.1:56001".to_string(),
            strm_access_token: "token".to_string(),
            ..Default::default()
        };
        let sub = subscription();

        let result = generate_subscription_strm_files(
            &settings,
            &sub,
            "/连续剧/庆余年（2024）/Season 1",
            &[video_item("fid1", "庆余年.S01E01.mkv")],
        )
        .unwrap();

        assert_eq!(result.generated_count, 1);
        assert!(result
            .output_dir
            .ends_with("连续剧/庆余年（2024）/Season 1"));
        let content = std::fs::read_to_string(&result.files[0].strm_path).unwrap();
        assert!(content.starts_with("http://127.0.0.1:56001/strm/quark/fid1/"));

        let _ = std::fs::remove_dir_all(root);
    }
}
