#![allow(dead_code)]

use once_cell::sync::Lazy;
use regex::Regex;

/// 视频扩展名
pub const VIDEO_EXTS: &[&str] = &[
    ".mkv", ".mp4", ".avi", ".ts", ".mov", ".wmv", ".flv", ".m4v", ".rmvb", ".webm",
];

struct EpisodePattern {
    regex: Regex,
}

/// 集数提取正则模式。明确格式优先，裸数字只作为兜底并过滤年份/清晰度。
static EPISODE_PATTERNS: Lazy<Vec<EpisodePattern>> = Lazy::new(|| {
    vec![
        EpisodePattern {
            regex: Regex::new(r"(?i)S(?P<season>\d{1,2})[._\-\s]*E(?P<episode>\d{1,4})").unwrap(),
        },
        EpisodePattern {
            regex: Regex::new(r"(?i)(?:^|[^\p{L}\d])EP?[._\-\s]*(?P<episode>\d{1,4})").unwrap(),
        },
        EpisodePattern {
            regex: Regex::new(r"第\s*(?P<episode>\d{1,4})\s*[集话話期]").unwrap(),
        },
        EpisodePattern {
            regex: Regex::new(r"[\[【]\s*(?P<episode>\d{1,4})\s*[\]】]").unwrap(),
        },
    ]
});

/// 集数检测结果
#[derive(Debug, Clone, PartialEq)]
pub struct EpisodeInfo {
    pub episode: Option<i32>,
    pub season: Option<i32>,
}

fn is_likely_explicit_episode_number(episode: i32) -> bool {
    episode > 0
}

fn is_likely_numeric_fallback_episode(episode: i32) -> bool {
    if episode <= 0 {
        return false;
    }
    if (1900..=2099).contains(&episode) {
        return false;
    }
    !matches!(episode, 480 | 720 | 1080 | 2160 | 4320)
}

fn numeric_fallback_episode(name: &str) -> Option<i32> {
    let stem = std::path::Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name);

    stem.split(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '.' | '_' | '-' | '[' | ']' | '(' | ')' | '【' | '】' | '（' | '）'
            )
    })
    .filter(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
    .filter_map(|part| part.parse::<i32>().ok())
    .find(|episode| is_likely_numeric_fallback_episode(*episode))
}

/// 是否是视频文件
pub fn is_video_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    VIDEO_EXTS.iter().any(|ext| lower.ends_with(ext))
}

/// 从文件名提取集数和季度
pub fn detect_episode(name: &str) -> EpisodeInfo {
    for pattern in EPISODE_PATTERNS.iter() {
        for caps in pattern.regex.captures_iter(name) {
            let episode = caps
                .name("episode")
                .and_then(|m| m.as_str().parse::<i32>().ok());
            let season = caps
                .name("season")
                .and_then(|m| m.as_str().parse::<i32>().ok());
            let season = if season == Some(0) { None } else { season };

            if !episode
                .map(is_likely_explicit_episode_number)
                .unwrap_or(false)
            {
                continue;
            }

            return EpisodeInfo { episode, season };
        }
    }

    if let Some(episode) = numeric_fallback_episode(name) {
        return EpisodeInfo {
            episode: Some(episode),
            season: None,
        };
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
    if !include_keywords.is_empty()
        && !include_keywords
            .iter()
            .any(|kw| lower.contains(&kw.to_lowercase()))
    {
        return false;
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
    fn test_detect_episode_s01e144_with_metadata() {
        let info = detect_episode("S01E144.2025.2160p.WEB-DL.HQ.H265.30fps.10bit.AAC.mp4");
        assert_eq!(info.episode, Some(144));
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
    fn test_detect_episode_number_with_quality_tag() {
        let info = detect_episode("129 4K.mp4");
        assert_eq!(info.episode, Some(129));
    }

    #[test]
    fn test_detect_episode_number_with_duplicate_suffix() {
        let info = detect_episode("23(1).mp4");
        assert_eq!(info.episode, Some(23));
    }

    #[test]
    fn test_detect_episode_skips_year_number() {
        let info = detect_episode("Movie.2024.mkv");
        assert_eq!(info.episode, None);
        assert_eq!(info.season, None);
    }

    #[test]
    fn test_detect_episode_skips_year_before_episode() {
        let info = detect_episode("Show.2025.129.4K.mp4");
        assert_eq!(info.episode, Some(129));
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
