#![allow(dead_code)]

use once_cell::sync::Lazy;
use regex::Regex;

/// 视频扩展名
pub const VIDEO_EXTS: &[&str] = &[
    ".mkv", ".mp4", ".avi", ".ts", ".mov", ".wmv", ".flv", ".m4v", ".rmvb", ".webm",
];

/// 集数提取正则模式（与 Python 一致）
static EPISODE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?i)S(?P<season>\d{1,2})E(?P<episode>\d{1,4})").unwrap(),
        Regex::new(r"(?i)EP?\s*(?P<episode>\d{1,4})").unwrap(),
        Regex::new(r"第\s*(?P<episode>\d{1,4})\s*[集话期]").unwrap(),
        Regex::new(r"(?i)[\[\[【第_\-\s\.](?P<episode>\d{1,4})[\]\]】_\-\s\.]").unwrap(),
        Regex::new(r"(?i)(?P<episode>\d{1,4})\.(mkv|mp4|avi|ts|mov|wmv|flv|m4v|rmvb|webm)$")
            .unwrap(),
    ]
});

/// 集数检测结果
#[derive(Debug, Clone, PartialEq)]
pub struct EpisodeInfo {
    pub episode: Option<i32>,
    pub season: Option<i32>,
}

/// 是否是视频文件
pub fn is_video_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    VIDEO_EXTS.iter().any(|ext| lower.ends_with(ext))
}

/// 从文件名提取集数和季度
pub fn detect_episode(name: &str) -> EpisodeInfo {
    for pattern in EPISODE_PATTERNS.iter() {
        if let Some(caps) = pattern.captures(name) {
            let episode = caps
                .name("episode")
                .and_then(|m| m.as_str().parse::<i32>().ok());
            let season = caps
                .name("season")
                .and_then(|m| m.as_str().parse::<i32>().ok());

            return EpisodeInfo {
                episode,
                season: if season == Some(0) { None } else { season },
            };
        }
    }

    EpisodeInfo {
        episode: None,
        season: None,
    }
}

/// 分割关键词（支持逗号、中文逗号、换行符）
pub fn split_words(value: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for v in value {
        for word in v.split(&[',', '，', '\n']) {
            let trimmed = word.trim();
            if !trimmed.is_empty() {
                result.push(trimmed.to_string());
            }
        }
    }
    result
}

/// 文件匹配（关键词 + 正则）
pub fn match_file(
    name: &str,
    include_keywords: &[String],
    exclude_keywords: &[String],
    regex: &str,
) -> bool {
    let lower = name.to_lowercase();

    // 包含关键词
    if !include_keywords.is_empty() {
        if !include_keywords
            .iter()
            .any(|kw| lower.contains(&kw.to_lowercase()))
        {
            return false;
        }
    }

    // 排除关键词
    if exclude_keywords
        .iter()
        .any(|kw| lower.contains(&kw.to_lowercase()))
    {
        return false;
    }

    // 正则匹配
    if !regex.is_empty() {
        match Regex::new(regex) {
            Ok(re) => {
                if !re.is_match(name) {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_video_name() {
        assert!(is_video_name("episode.mkv"));
        assert!(is_video_name("MOVIE.MP4"));
        assert!(!is_video_name("subtitle.srt"));
    }

    #[test]
    fn test_detect_episode_s01e01() {
        let info = detect_episode("Show.S01E05.1080p.mkv");
        assert_eq!(info.episode, Some(5));
        assert_eq!(info.season, Some(1));
    }

    #[test]
    fn test_detect_episode_chinese() {
        let info = detect_episode("某动画 第12集.mkv");
        assert_eq!(info.episode, Some(12));
        assert_eq!(info.season, None);
    }

    #[test]
    fn test_detect_episode_ep() {
        let info = detect_episode("[字幕组] EP08.mp4");
        assert_eq!(info.episode, Some(8));
    }

    #[test]
    fn test_detect_episode_number_only() {
        let info = detect_episode("03.mkv");
        assert_eq!(info.episode, Some(3));
    }

    #[test]
    fn test_detect_episode_none() {
        let info = detect_episode("预告.mp4");
        assert_eq!(info.episode, None);
        assert_eq!(info.season, None);
    }

    #[test]
    fn test_split_words() {
        let input = vec![
            "关键词1,关键词2".to_string(),
            "关键词3，关键词4".to_string(),
        ];
        let result = split_words(&input);
        assert_eq!(result, vec!["关键词1", "关键词2", "关键词3", "关键词4"]);
    }

    #[test]
    fn test_match_file_include() {
        assert!(match_file(
            "某字幕组.第01集.mkv",
            &["字幕组".to_string()],
            &[],
            ""
        ));
        assert!(!match_file(
            "某字幕组.第01集.mkv",
            &["其他".to_string()],
            &[],
            ""
        ));
    }

    #[test]
    fn test_match_file_exclude() {
        assert!(!match_file("预告片.mkv", &[], &["预告".to_string()], ""));
        assert!(match_file("正片.mkv", &[], &["预告".to_string()], ""));
    }

    #[test]
    fn test_match_file_regex() {
        assert!(match_file("E01.mkv", &[], &[], r"E\d{2}"));
        assert!(!match_file("E01.mkv", &[], &[], r"E\d{3}"));
    }
}
