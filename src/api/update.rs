use axum::{
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};

const GITHUB_REPO: &str = "hellomrli/my-media-sub";

#[derive(Serialize)]
struct Response<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
}

impl<T> Response<T> {
    fn ok(data: T) -> Self {
        Self { data: Some(data) }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    body: Option<String>,
    published_at: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GithubAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateAsset {
    pub name: String,
    pub size: u64,
    pub download_url: String,
}

impl From<GithubAsset> for UpdateAsset {
    fn from(asset: GithubAsset) -> Self {
        Self {
            name: asset.name,
            size: asset.size,
            download_url: asset.browser_download_url,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UpdateCheckResponse {
    pub repository: String,
    pub current_version: String,
    pub latest_version: String,
    pub latest_tag: String,
    pub update_available: bool,
    pub release_name: String,
    pub release_url: String,
    pub release_notes: String,
    pub published_at: Option<String>,
    pub checked_at: String,
    pub runtime: String,
    pub linux_x86_64_asset: Option<UpdateAsset>,
}

#[derive(Debug, Serialize)]
pub struct UpdateApplyResponse {
    pub success: bool,
    pub previous_version: String,
    pub new_version: String,
    pub binary_path: String,
    pub backup_path: String,
    pub restart_required: bool,
    pub message: String,
}

async fn check_update() -> Result<impl IntoResponse> {
    let release = fetch_latest_release().await?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let latest_version = normalize_version(&release.tag_name);
    let update_available = is_newer_version(&latest_version, &current_version);
    let linux_x86_64_asset = find_asset(&release.assets, "linux-x86_64.tar.gz").map(Into::into);

    let response = UpdateCheckResponse {
        repository: GITHUB_REPO.to_string(),
        current_version,
        latest_version,
        latest_tag: release.tag_name.clone(),
        update_available,
        release_name: release.name.unwrap_or_else(|| release.tag_name.clone()),
        release_url: release.html_url,
        release_notes: release.body.unwrap_or_default(),
        published_at: release.published_at,
        checked_at: Utc::now().to_rfc3339(),
        runtime: detect_runtime(),
        linux_x86_64_asset,
    };

    Ok(Json(Response::ok(response)))
}

async fn apply_update() -> Result<impl IntoResponse> {
    let release = fetch_latest_release().await?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let latest_version = normalize_version(&release.tag_name);
    if !is_newer_version(&latest_version, &current_version) {
        return Err(AppError::Validation("当前已是最新版本".to_string()));
    }

    let asset = find_asset(&release.assets, "linux-x86_64.tar.gz")
        .ok_or_else(|| AppError::NotFound("Release 中未找到 Linux x86_64 二进制包".to_string()))?;
    let current_exe = std::env::current_exe()
        .map_err(|e| AppError::Internal(format!("无法定位当前二进制: {}", e)))?;
    let backup_path = backup_path(&current_exe);
    let work_dir = std::env::temp_dir().join(format!(
        "my-media-sub-update-{}-{}",
        latest_version,
        uuid::Uuid::new_v4()
    ));
    tokio::fs::create_dir_all(&work_dir)
        .await
        .map_err(|e| AppError::Internal(format!("创建升级临时目录失败: {}", e)))?;

    let archive_path = work_dir.join(&asset.name);
    download_asset(&asset.browser_download_url, &archive_path).await?;
    extract_archive(&archive_path, &work_dir).await?;
    let new_binary = find_binary(&work_dir)
        .ok_or_else(|| AppError::Internal("升级包中未找到 my-media-sub 二进制".to_string()))?;
    replace_binary(&new_binary, &current_exe, &backup_path).await?;

    let _ = tokio::fs::remove_dir_all(&work_dir).await;

    Ok(Json(Response::ok(UpdateApplyResponse {
        success: true,
        previous_version: current_version,
        new_version: latest_version,
        binary_path: current_exe.display().to_string(),
        backup_path: backup_path.display().to_string(),
        restart_required: true,
        message: "二进制已替换，重启服务后生效".to_string(),
    })))
}

async fn fetch_latest_release() -> Result<GithubRelease> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-update-check")
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<GithubRelease>().await?)
}

fn detect_runtime() -> String {
    if std::path::Path::new("/.dockerenv").exists() {
        "docker".to_string()
    } else {
        "binary".to_string()
    }
}

async fn download_asset(url: &str, path: &Path) -> Result<()> {
    let response = reqwest::Client::new()
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-self-update")
        .send()
        .await?
        .error_for_status()?;
    let bytes = response.bytes().await?;
    tokio::fs::write(path, bytes)
        .await
        .map_err(|e| AppError::Internal(format!("写入升级包失败: {}", e)))
}

async fn extract_archive(archive_path: &Path, output_dir: &Path) -> Result<()> {
    let archive_path = archive_path.to_path_buf();
    let output_dir = output_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new("tar")
            .arg("-xzf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&output_dir)
            .output()
            .map_err(|e| AppError::Internal(format!("执行 tar 解压失败: {}", e)))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(AppError::Internal(format!(
                "解压升级包失败: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    })
    .await
    .map_err(|e| AppError::Internal(format!("解压任务失败: {}", e)))?
}

fn backup_path(current_exe: &Path) -> PathBuf {
    let file_name = current_exe
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("my-media-sub");
    current_exe.with_file_name(format!(
        "{}.bak-{}",
        file_name,
        Utc::now().format("%Y%m%d%H%M%S")
    ))
}

async fn replace_binary(new_binary: &Path, current_exe: &Path, backup_path: &Path) -> Result<()> {
    tokio::fs::copy(current_exe, backup_path)
        .await
        .map_err(|e| AppError::Internal(format!("备份当前二进制失败: {}", e)))?;
    let metadata = tokio::fs::metadata(current_exe)
        .await
        .map_err(|e| AppError::Internal(format!("读取当前二进制权限失败: {}", e)))?;
    let staging_path = current_exe.with_file_name(format!(
        ".{}.new-{}",
        current_exe
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("my-media-sub"),
        uuid::Uuid::new_v4()
    ));
    tokio::fs::copy(new_binary, &staging_path)
        .await
        .map_err(|e| AppError::Internal(format!("写入新二进制失败: {}", e)))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(
            &staging_path,
            std::fs::Permissions::from_mode(metadata.permissions().mode()),
        )
        .await
        .map_err(|e| AppError::Internal(format!("设置新二进制权限失败: {}", e)))?;
    }

    tokio::fs::rename(&staging_path, current_exe)
        .await
        .map_err(|e| AppError::Internal(format!("替换当前二进制失败: {}", e)))?;

    Ok(())
}

fn find_binary(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries = std::fs::read_dir(path).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == "my-media-sub")
                .unwrap_or(false)
            {
                return Some(path);
            }
        }
    }
    None
}

fn normalize_version(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string()
}

fn find_asset(assets: &[GithubAsset], suffix: &str) -> Option<GithubAsset> {
    assets
        .iter()
        .find(|asset| asset.name.ends_with(suffix))
        .cloned()
}

fn is_newer_version(candidate: &str, current: &str) -> bool {
    compare_versions(candidate, current) == Some(Ordering::Greater)
}

fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    if left_parts.is_empty() || right_parts.is_empty() {
        return None;
    }

    for index in 0..left_parts.len().max(right_parts.len()) {
        let left_value = left_parts.get(index).copied().unwrap_or(0);
        let right_value = right_parts.get(index).copied().unwrap_or(0);
        match left_value.cmp(&right_value) {
            Ordering::Equal => {}
            ordering => return Some(ordering),
        }
    }

    Some(Ordering::Equal)
}

fn version_parts(value: &str) -> Vec<u64> {
    normalize_version(value)
        .split(['.', '-', '+'])
        .filter_map(|part| {
            let digits: String = part.chars().take_while(|ch| ch.is_ascii_digit()).collect();
            if digits.is_empty() {
                None
            } else {
                digits.parse::<u64>().ok()
            }
        })
        .collect()
}

pub fn routes() -> Router {
    Router::new()
        .route("/api/update/check", get(check_update))
        .route("/api/update/apply", post(apply_update))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> GithubAsset {
        GithubAsset {
            name: name.to_string(),
            size: 42,
            browser_download_url: format!("https://example.com/{}", name),
        }
    }

    #[test]
    fn test_version_compare_handles_tags() {
        assert!(is_newer_version("v0.7.15", "0.7.14"));
        assert!(is_newer_version("0.8.0", "0.7.99"));
        assert!(!is_newer_version("0.7.14", "0.7.14"));
        assert!(!is_newer_version("0.7.13", "0.7.14"));
    }

    #[test]
    fn test_find_release_assets() {
        let assets = vec![
            asset("my-media-sub-v0.7.15-linux-x86_64.tar.gz"),
            asset("my-media-sub-v0.7.15-linux-x86_64.tar.gz.sha256"),
        ];

        let archive = find_asset(&assets, "linux-x86_64.tar.gz").unwrap();
        let checksum = find_asset(&assets, "linux-x86_64.tar.gz.sha256").unwrap();

        assert_eq!(archive.name, "my-media-sub-v0.7.15-linux-x86_64.tar.gz");
        assert_eq!(
            checksum.name,
            "my-media-sub-v0.7.15-linux-x86_64.tar.gz.sha256"
        );
    }
}
