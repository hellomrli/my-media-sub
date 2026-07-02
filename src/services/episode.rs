#![allow(dead_code)]

use regex::Regex;
use std::sync::LazyLock;

/// 视频扩展名
pub const VIDEO_EXTS: &[&str] = &[
    ".mkv", ".mp4", ".avi", ".ts", ".mov", ".wmv", ".flv", ".m4v", ".rmvb", ".webm",
];

struct EpisodePattern {
    regex: Regex,
}

#[derive(Debug, Clone, Copy)]
pub struct EpisodeDuplicateCandidate<'a> {
    pub name: &'a str,
    pub size: i64,
    pub updated_at: Option<&'a str>,
    pub order: usize,
}

fn hardcoded_regex(pattern: &str) -> Regex {
    Regex::new(pattern)
        .unwrap_or_else(|error| panic!("invalid hard-coded episode regex `{pattern}`: {error}"))
}

/// 集数提取正则模式。明确格式优先，裸数字只作为兜底并过滤年份/清晰度。
static EPISODE_PATTERNS: LazyLock<Vec<EpisodePattern>> = LazyLock::new(|| {
    vec![
        EpisodePattern {
            regex: hardcoded_regex(r"(?i)S(?P<season>\d{1,2})[._\-\s]*E(?P<episode>\d{1,4})"),
        },
        EpisodePattern {
            regex: hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])EP?[._\-\s]*(?P<episode>\d{1,4})"),
        },
        EpisodePattern {
            regex: hardcoded_regex(r"第\s*(?P<episode>\d{1,4})\s*[集话話期]"),
        },
        EpisodePattern {
            regex: hardcoded_regex(r"[\[【]\s*(?P<episode>\d{1,4})\s*[\]】]"),
        },
    ]
});

static QUALITY_PATTERNS: LazyLock<Vec<(Regex, i64)>> = LazyLock::new(|| {
    vec![
        (
            hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])(?:8k|4320p)(?:$|[^\p{L}\d])"),
            4320,
        ),
        (
            hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])(?:4k|2160p)(?:$|[^\p{L}\d])"),
            2160,
        ),
        (
            hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])(?:2k|1440p)(?:$|[^\p{L}\d])"),
            1440,
        ),
        (
            hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])1080p(?:$|[^\p{L}\d])"),
            1080,
        ),
        (
            hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])720p(?:$|[^\p{L}\d])"),
            720,
        ),
        (
            hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])480p(?:$|[^\p{L}\d])"),
            480,
        ),
    ]
});

static SEASON_HINT_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        hardcoded_regex(r"(?i)S(?P<num>\d{1,2})[._\-\s]*E\d{1,4}"),
        hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])S(?P<num>\d{1,2})(?:$|[^\p{L}\d])"),
        hardcoded_regex(r"(?i)(?:season|series)[._\-\s]*(?P<num>\d{1,2})"),
        hardcoded_regex(r"第\s*(?P<num>\d{1,2})\s*季"),
        hardcoded_regex(r"第\s*(?P<cn>[一二三四五六七八九十两]+)\s*季"),
    ]
});

static NON_CURRENT_COLLECTION_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        hardcoded_regex(r"(?i)(番外|剧场版|剧场|特别篇|特别版|special|ova|oad)"),
        hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])sp(?:$|[^\p{L}\d])"),
        hardcoded_regex(r"前\s*(?:\d+|[一二三四五六七八九十两]+)\s*季"),
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
    .or_else(|| leading_numeric_episode(stem))
}

fn leading_numeric_episode(stem: &str) -> Option<i32> {
    let digit_end = stem
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit())
        .last()
        .map(|(index, ch)| index + ch.len_utf8())?;

    let suffix = stem[digit_end..].trim_start();
    if suffix
        .chars()
        .next()
        .map(|ch| matches!(ch, 'p' | 'P' | 'k' | 'K'))
        .unwrap_or(false)
    {
        return None;
    }

    stem[..digit_end]
        .parse::<i32>()
        .ok()
        .filter(|episode| is_likely_numeric_fallback_episode(*episode))
}

fn chinese_digit_value(ch: char) -> Option<i32> {
    match ch {
        '零' | '〇' => Some(0),
        '一' => Some(1),
        '二' | '两' => Some(2),
        '三' => Some(3),
        '四' => Some(4),
        '五' => Some(5),
        '六' => Some(6),
        '七' => Some(7),
        '八' => Some(8),
        '九' => Some(9),
        _ => None,
    }
}

fn parse_chinese_number(value: &str) -> Option<i32> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value == "十" {
        return Some(10);
    }

    if let Some((left, right)) = value.split_once('十') {
        let tens = if left.is_empty() {
            1
        } else {
            left.chars().next().and_then(chinese_digit_value)?
        };
        let ones = if right.is_empty() {
            0
        } else {
            right.chars().next().and_then(chinese_digit_value)?
        };
        return Some(tens * 10 + ones);
    }

    value.chars().next().and_then(chinese_digit_value)
}

pub fn season_hint_from_text(value: &str) -> Option<i32> {
    for pattern in SEASON_HINT_PATTERNS.iter() {
        for caps in pattern.captures_iter(value) {
            if let Some(num) = caps
                .name("num")
                .and_then(|m| m.as_str().parse::<i32>().ok())
            {
                return Some(num);
            }
            if let Some(num) = caps
                .name("cn")
                .and_then(|m| parse_chinese_number(m.as_str()))
            {
                return Some(num);
            }
        }
    }

    None
}

pub fn season_hint_from_context(name: &str, parent_path: &str) -> Option<i32> {
    season_hint_from_text(name).or_else(|| {
        parent_path
            .rsplit('/')
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .find_map(season_hint_from_text)
    })
}

pub fn has_non_current_collection_hint(parent_path: &str) -> bool {
    if parent_path.trim().is_empty() {
        return false;
    }
    NON_CURRENT_COLLECTION_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(parent_path))
}

pub fn matches_subscription_season(
    name: &str,
    parent_path: &str,
    subscription_season: i32,
) -> bool {
    let expected = subscription_season.max(1);
    if let Some(season) = season_hint_from_context(name, parent_path) {
        return season == expected;
    }
    !has_non_current_collection_hint(parent_path)
}

/// 是否是视频文件
pub fn is_video_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    VIDEO_EXTS.iter().any(|ext| lower.ends_with(ext))
}

/// 用于同一订阅内按季度和集数识别同集视频。
pub fn episode_video_key(name: &str, default_season: i32) -> Option<(i32, i32)> {
    if !is_video_name(name) {
        return None;
    }

    let info = detect_episode(name);
    let episode = info.episode?;
    let season = info.season.unwrap_or(default_season).max(1);
    Some((season, episode))
}

pub fn normalize_duplicate_episode_strategy(strategy: &str) -> &'static str {
    match strategy.trim().to_ascii_lowercase().as_str() {
        "latest_upload" | "latest_uploaded" | "latest_time" | "latest" | "newest" => {
            "latest_upload"
        }
        "largest_size" | "size" | "biggest" => "largest_size",
        "first" | "first_seen" => "first",
        _ => "highest_quality",
    }
}

pub fn episode_quality_score(name: &str) -> i64 {
    QUALITY_PATTERNS
        .iter()
        .find_map(|(regex, score)| regex.is_match(name).then_some(*score))
        .unwrap_or(0)
}

pub fn parse_file_time_score(value: Option<&str>) -> i64 {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return 0;
    };

    if let Ok(timestamp) = value.parse::<i64>() {
        return if timestamp > 10_000_000_000 {
            timestamp / 1000
        } else {
            timestamp
        };
    }

    if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(value) {
        return datetime.timestamp();
    }

    chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .map(|datetime| datetime.and_utc().timestamp())
        .unwrap_or(0)
}

fn duplicate_candidate_scores(
    candidate: EpisodeDuplicateCandidate<'_>,
    strategy: &str,
) -> [i64; 4] {
    let quality = episode_quality_score(candidate.name);
    let time = parse_file_time_score(candidate.updated_at);
    let size = candidate.size.max(0);
    let first_order = -(candidate.order as i64);

    match normalize_duplicate_episode_strategy(strategy) {
        "latest_upload" => [time, quality, size, first_order],
        "largest_size" => [size, quality, time, first_order],
        "first" => [first_order, quality, size, time],
        _ => [quality, size, time, first_order],
    }
}

pub fn is_better_episode_duplicate_candidate(
    candidate: EpisodeDuplicateCandidate<'_>,
    current: EpisodeDuplicateCandidate<'_>,
    strategy: &str,
) -> bool {
    duplicate_candidate_scores(candidate, strategy) > duplicate_candidate_scores(current, strategy)
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
    fn test_detect_episode_number_with_suffix() {
        let info = detect_episode("178重置版.mp4");
        assert_eq!(info.episode, Some(178));
        assert_eq!(info.season, None);
    }

    #[test]
    fn test_detect_episode_real_world_numeric_variants() {
        let cases = [
            ("001v2.mp4", Some(1)),
            ("第178话 重置版.mp4", Some(178)),
            ("179 V2 1080p.mp4", Some(179)),
            ("S01 - 178 重制版.mkv", Some(178)),
            ("E178v2.mp4", Some(178)),
            ("2024重置版.mp4", None),
            ("2160p重置版.mp4", None),
        ];

        for (name, expected) in cases {
            let info = detect_episode(name);
            assert_eq!(info.episode, expected, "failed to parse {name}");
        }
    }

    #[test]
    fn test_detect_episode_skips_quality_only_name() {
        let info = detect_episode("4K.mp4");
        assert_eq!(info.episode, None);

        let info = detect_episode("1080p.mp4");
        assert_eq!(info.episode, None);
    }

    #[test]
    fn test_episode_video_key_uses_numeric_fallback_and_default_season() {
        assert_eq!(episode_video_key("178-4k.mkv", 1), Some((1, 178)));
        assert_eq!(episode_video_key("178重置版.mp4", 1), Some((1, 178)));
        assert_eq!(episode_video_key("Show.S02E178.mkv", 1), Some((2, 178)));
        assert_eq!(episode_video_key("178.ass", 1), None);
    }

    #[test]
    fn test_matches_subscription_season_uses_parent_path_context() {
        assert!(matches_subscription_season("178重置版.mp4", "", 6));
        assert!(matches_subscription_season(
            "25 4K.mp4",
            "一人之下 第六季/第6季",
            6
        ));
        assert!(!matches_subscription_season(
            "01.mp4",
            "前五季+番外+剧场版/第1季（2016）4K",
            6
        ));
        assert!(!matches_subscription_season(
            "S03E01.2020.1080p.WEB-DL.H265.mp4",
            "",
            6
        ));
        assert!(!matches_subscription_season(
            "4K.mp4",
            "前五季+番外+剧场版/锈铁重现（2024）4K",
            6
        ));
    }

    #[test]
    fn test_duplicate_episode_candidate_prefers_highest_quality_by_default() {
        let current = EpisodeDuplicateCandidate {
            name: "178.mkv",
            size: 2,
            updated_at: None,
            order: 0,
        };
        let candidate = EpisodeDuplicateCandidate {
            name: "178-4k.mkv",
            size: 1,
            updated_at: None,
            order: 1,
        };

        assert!(is_better_episode_duplicate_candidate(
            candidate,
            current,
            "highest_quality"
        ));
    }

    #[test]
    fn test_duplicate_episode_candidate_can_prefer_latest_upload() {
        let current = EpisodeDuplicateCandidate {
            name: "178-4k.mkv",
            size: 2,
            updated_at: Some("2024-01-01T00:00:00Z"),
            order: 0,
        };
        let candidate = EpisodeDuplicateCandidate {
            name: "178.mkv",
            size: 1,
            updated_at: Some("2024-01-02T00:00:00Z"),
            order: 1,
        };

        assert!(is_better_episode_duplicate_candidate(
            candidate,
            current,
            "latest_upload"
        ));
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
