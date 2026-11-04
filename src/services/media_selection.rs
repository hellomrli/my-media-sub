use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::error::{AppError, Result};
use crate::models::subscription::ProbeFile;

static RESOLUTIONS: LazyLock<Vec<(&str, Regex)>> = LazyLock::new(|| {
    [
        (
            "4320p",
            r"(?i)(?:^|[^a-z0-9])(?:4320[pi]|8k|7680\s*[x*\u{00d7}]\s*4320)(?:$|[^a-z0-9])",
        ),
        (
            "2160p",
            r"(?i)(?:^|[^a-z0-9])(?:2160[pi]|4k|uhd|3840\s*[x*\u{00d7}]\s*2160)(?:$|[^a-z0-9])",
        ),
        (
            "1440p",
            r"(?i)(?:^|[^a-z0-9])(?:1440[pi]|2k|2560\s*[x*\u{00d7}]\s*1440)(?:$|[^a-z0-9])",
        ),
        (
            "1080p",
            r"(?i)(?:^|[^a-z0-9])(?:1080[pi]|fhd|1920\s*[x*\u{00d7}]\s*1080)(?:$|[^a-z0-9])",
        ),
        (
            "720p",
            r"(?i)(?:^|[^a-z0-9])(?:720[pi]|1280\s*[x*\u{00d7}]\s*720)(?:$|[^a-z0-9])",
        ),
        (
            "480p",
            r"(?i)(?:^|[^a-z0-9])(?:480[pi]|576[pi]|sd)(?:$|[^a-z0-9])",
        ),
    ]
    .into_iter()
    .map(|(id, pattern)| (id, Regex::new(pattern).expect("valid resolution pattern")))
    .collect()
});

pub fn normalize_resolution(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    let normalized = match value.as_str() {
        "" | "auto" => "",
        "8k" | "4320p" | "4320i" => "4320p",
        "4k" | "uhd" | "2160p" | "2160i" => "2160p",
        "2k" | "1440p" | "1440i" => "1440p",
        "fhd" | "1080p" | "1080i" => "1080p",
        "720p" | "720i" => "720p",
        "480p" | "480i" | "576p" | "576i" | "sd" => "480p",
        "unknown" => "unknown",
        _ => return Err(AppError::Validation("不支持的画质选项".to_string())),
    };
    Ok(normalized.to_string())
}

pub fn resolution_label(resolution: &str) -> &'static str {
    match resolution {
        "4320p" => "8K",
        "2160p" => "4K / 2160P",
        "1440p" => "2K / 1440P",
        "1080p" => "1080P",
        "720p" => "720P",
        "480p" => "标清",
        "unknown" => "未标注画质",
        _ => "自动选择",
    }
}

fn resolution_in_text(text: &str) -> Option<&'static str> {
    let mut matches = RESOLUTIONS
        .iter()
        .filter_map(|(id, pattern)| pattern.is_match(text).then_some(*id));
    let first = matches.next()?;
    // 分享的合集目录可能同时写了 1080P 和 4K，不能把它当成单个文件的画质。
    matches.next().is_none().then_some(first)
}

pub fn file_resolution(name: &str, parent_path: &str) -> &'static str {
    resolution_in_text(name)
        .or_else(|| parent_path.rsplit(['/', '\\']).find_map(resolution_in_text))
        .unwrap_or("unknown")
}

pub fn matches_resolution(preferred: &str, name: &str, parent_path: &str) -> bool {
    match normalize_resolution(preferred) {
        Ok(preferred) => preferred.is_empty() || preferred == file_resolution(name, parent_path),
        Err(_) => false,
    }
}

pub fn is_main_video(name: &str) -> bool {
    if !crate::services::is_video_name(name) {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    !["预告", "花絮", "解说", "彩蛋", "trailer", "preview"]
        .iter()
        .any(|word| lower.contains(word))
        && !lower
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|token| token == "sample")
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DetectedQuality {
    pub resolution: String,
    pub label: String,
    pub file_count: usize,
    pub total_size: i64,
    pub sample_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DetectedMediaFile {
    pub file_key: String,
    pub name: String,
    pub parent_path: String,
    pub size: i64,
    pub resolution: String,
}

pub fn analyze_qualities(files: &[ProbeFile]) -> (Vec<DetectedQuality>, Vec<DetectedMediaFile>) {
    let mut groups: BTreeMap<&str, DetectedQuality> = BTreeMap::new();
    let mut media_files = Vec::new();
    for file in files {
        if file.is_dir || !is_main_video(&file.name) {
            continue;
        }
        let resolution = file_resolution(&file.name, &file.parent_path);
        let group = groups.entry(resolution).or_insert_with(|| DetectedQuality {
            resolution: resolution.to_string(),
            label: resolution_label(resolution).to_string(),
            file_count: 0,
            total_size: 0,
            sample_files: Vec::new(),
        });
        group.file_count += 1;
        group.total_size = group.total_size.saturating_add(file.size.max(0));
        if group.sample_files.len() < 3 {
            group.sample_files.push(if file.parent_path.is_empty() {
                file.name.clone()
            } else {
                format!("{}/{}", file.parent_path, file.name)
            });
        }
        media_files.push(DetectedMediaFile {
            file_key: file.file_key.clone(),
            name: file.name.clone(),
            parent_path: file.parent_path.clone(),
            size: file.size.max(0),
            resolution: resolution.to_string(),
        });
    }
    let order = [
        "4320p", "2160p", "1440p", "1080p", "720p", "480p", "unknown",
    ];
    let qualities = order.iter().filter_map(|id| groups.remove(id)).collect();
    (qualities, media_files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_names_and_nearest_quality_folder() {
        for (name, path, expected) in [
            ("Movie.2160p.HDR.mkv", "1080P", "2160p"),
            ("Movie.mkv", "电影合集/4K 高码率", "2160p"),
            ("01.mkv", "1080P & 4K/4K/Season 1", "2160p"),
            ("01.mkv", "4K/1080P/Season 2", "1080p"),
            ("01.mkv", "Show\\1080P\\Season 1", "1080p"),
            ("Movie.1920x1080.mkv", "", "1080p"),
            ("Movie.FHD.mkv", "", "1080p"),
            ("Movie.mkv", "1080P & 4K", "unknown"),
            ("Movie.HDR.H265.mkv", "", "unknown"),
            ("Movie.2024.mkv", "", "unknown"),
            ("Movie.4kids.mkv", "", "unknown"),
        ] {
            assert_eq!(file_resolution(name, path), expected, "{name} in {path}");
        }
    }

    #[test]
    fn explicit_quality_never_falls_back_to_another_version() {
        assert!(matches_resolution("4K", "01.mkv", "4K/Season 1"));
        assert!(!matches_resolution("1080p", "01.mkv", "4K/Season 1"));
        assert!(!matches_resolution("1080p", "01.mkv", "Season 1"));
        assert!(matches_resolution("unknown", "01.mkv", "Season 1"));
        assert!(matches_resolution("", "01.mkv", "Season 1"));
        assert!(!matches_resolution("invalid", "01.mkv", "Season 1"));
    }

    #[test]
    fn groups_quality_options_with_paths_sizes_and_exact_file_ids() {
        let files = [
            ("Movie.mkv", "4K", 20, "4k"),
            ("Movie.mkv", "1080P", 5, "hd"),
            ("Movie.Directors.Cut.mkv", "4K", 25, "cut"),
            ("Movie.trailer.mkv", "4K", 1, "trailer"),
            ("Movie.sample.mkv", "4K", 1, "sample"),
            ("poster.jpg", "4K", 1, "poster"),
        ]
        .into_iter()
        .map(|(name, path, size, key)| ProbeFile {
            name: name.to_string(),
            parent_path: path.to_string(),
            size,
            file_key: key.to_string(),
            is_dir: false,
            updated_at: None,
        })
        .collect::<Vec<_>>();
        let (qualities, media_files) = analyze_qualities(&files);
        assert_eq!(qualities.len(), 2);
        assert_eq!(qualities[0].resolution, "2160p");
        assert_eq!(qualities[0].file_count, 2);
        assert_eq!(qualities[0].total_size, 45);
        assert_eq!(qualities[1].resolution, "1080p");
        assert_eq!(media_files.len(), 3);
        assert_eq!(media_files[0].file_key, "4k");
        assert_eq!(media_files[1].file_key, "hd");
    }
}
