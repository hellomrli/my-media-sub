use regex::Regex;
use std::sync::LazyLock;

/// 视频扩展名
pub const VIDEO_EXTS: &[&str] = &[
    ".mkv", ".mp4", ".avi", ".ts", ".mov", ".wmv", ".flv", ".m4v", ".rmvb", ".webm",
];

struct EpisodePattern {
    id: &'static str,
    regex: Regex,
    /// 方括号里的 4 位数字绝大多数是年份（[SubGroup][2024][05]），
    /// 该模式命中 1900–2099 时继续扫描后续候选而不是直接采信。
    reject_year: bool,
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
            id: "season_episode",
            regex: hardcoded_regex(r"(?i)S(?P<season>\d{1,2})[._\-\s]*E(?P<episode>\d{1,4})"),
            reject_year: false,
        },
        EpisodePattern {
            id: "episode_marker",
            regex: hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])EP?[._\-\s]*(?P<episode>\d{1,4})"),
            reject_year: false,
        },
        EpisodePattern {
            id: "special_marker",
            regex: hardcoded_regex(
                r"(?i)(?:^|[^\p{L}\d])(?:SP|OVA|OAD)[._\-\s]*(?P<episode>\d{1,4})(?:$|[^\p{L}\d])",
            ),
            reject_year: false,
        },
        EpisodePattern {
            id: "chinese_episode",
            regex: hardcoded_regex(r"第\s*(?P<episode>\d{1,4})\s*[集话話期]"),
            reject_year: false,
        },
        EpisodePattern {
            id: "bracket_number",
            regex: hardcoded_regex(r"[\[【]\s*(?P<episode>\d{1,4})\s*[\]】]"),
            reject_year: true,
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

/// Explainable episode detection used by previews and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeDetection {
    pub episode: Option<i32>,
    pub episodes: Vec<i32>,
    pub season: Option<i32>,
    pub special_kind: Option<&'static str>,
    pub method: &'static str,
    pub confidence: &'static str,
    pub reason: String,
}

fn is_likely_explicit_episode_number(episode: i32) -> bool {
    episode > 0
}

fn is_plausible_year(episode: i32) -> bool {
    (1900..=2099).contains(&episode)
}

fn special_kind_for(name: &str) -> Option<&'static str> {
    let upper = name.to_ascii_uppercase();
    if upper.contains("OVA") {
        Some("ova")
    } else if upper.contains("OAD") {
        Some("oad")
    } else {
        Some("sp")
    }
}

/// numeric_fallback 前从文件名剥离的编码/声道噪声（x265、H.264、DD5.1 等），
/// 否则 "Movie.2024.H.265" 会把 265 当成集数、"DD5.1" 会把 1 当成集数。
static FALLBACK_NOISE_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])(?:x|h)[._\-\s]*26[0-9](?:$|[^\p{L}\d])"),
        hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])(?:hevc|avc|xvid|divx)(?:$|[^\p{L}\d])"),
        hardcoded_regex(
            r"(?i)(?:^|[^\p{L}\d])(?:dd|ddp|dd+|eac-?3|ac-?3|dts(?:[._\-\s]*hd)?|truehd|atmos|flac|aac|opus)(?:[._\-\s]*\d)?(?:[._]\d)?(?:ch)?(?:$|[^\p{L}\d])",
        ),
    ]
});

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

    // 剥离编码/声道噪声后再做独立数字兜底，避免 265/1 这类技术数字误判。
    let stripped = FALLBACK_NOISE_PATTERNS
        .iter()
        .fold(stem.to_string(), |acc, pattern| {
            pattern.replace_all(&acc, "").to_string()
        });

    stripped
        .split(|ch: char| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '.' | '_' | '-' | '[' | ']' | '(' | ')' | '【' | '】' | '（' | '）'
                )
        })
        .filter(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
        .filter_map(|part| part.parse::<i32>().ok())
        .find(|episode| is_likely_numeric_fallback_episode(*episode))
        .or_else(|| leading_numeric_episode(&stripped))
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
    matches_subscription_season_range(name, parent_path, subscription_season, subscription_season)
}

/// 判断文件是否属于订阅的季范围 `[start, end]`（闭区间）。
///
/// - 能从文件名/父路径识别季号时：必须落在范围内；
/// - 无季号提示时：排除明显非本季合集路径后接受（多季会回落到起始季转存）。
pub fn matches_subscription_season_range(
    name: &str,
    parent_path: &str,
    season_start: i32,
    season_end: i32,
) -> bool {
    let start = season_start.max(1);
    let end = season_end.max(start);
    if let Some(season) = season_hint_from_context(name, parent_path) {
        return season >= start && season <= end;
    }
    !has_non_current_collection_hint(parent_path)
}

/// 解析文件所属季号：优先文件名/路径提示，否则回落到 `default_season`（多季通常为起始季）。
pub fn resolve_file_season(
    name: &str,
    parent_path: &str,
    default_season: i32,
    _multi_season: bool,
) -> Option<i32> {
    if let Some(season) = season_hint_from_context(name, parent_path) {
        return Some(season.max(1));
    }
    Some(default_season.max(1))
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

/// 与 special_marker 同源的模式，供流水线判断「这是特典文件」。
static SPECIAL_MARKER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    hardcoded_regex(r"(?i)(?:^|[^\p{L}\d])(?:SP|OVA|OAD)[._\-\s]*\d{1,4}(?:$|[^\p{L}\d])")
});

/// 是否是特典文件（SP/OVA/OAD 编号）。
pub fn is_special_episode_name(name: &str) -> bool {
    SPECIAL_MARKER_REGEX.is_match(name)
}

/// 检查/转存流水线的同集状态键。
///
/// 与 `episode_video_key` 的区别：特典（SP/OVA/OAD）返回 `None`，只参与按
/// 文件名的去重与幂等，不占用正片集数槽位——否则 SP01 会以 ep:1 的身份
/// 与正片 EP01 互相挤掉。展示与诊断仍使用 `detect_episode_explained`。
pub fn episode_state_key(name: &str, default_season: i32) -> Option<(i32, i32)> {
    if is_special_episode_name(name) {
        return None;
    }
    episode_video_key(name, default_season)
}

/// 同 `episode_state_key`，但优先使用订阅级 episode_regex 覆盖。
///
/// 自动检查/转存/去重链路必须与展示预览使用同一识别结果，否则配置了
/// 覆盖正则的订阅在预览里看得到集数、在自动链路里却被判「无法识别」。
/// 覆盖正则无效或未命中该文件时安全回落到默认识别。
pub fn episode_state_key_with_override(
    name: &str,
    default_season: i32,
    override_regex: &str,
) -> Option<(i32, i32)> {
    if is_special_episode_name(name) {
        return None;
    }
    if !is_video_name(name) {
        return None;
    }
    let detected = match detect_episode_with_override(name, override_regex) {
        Ok(detected) => detected,
        Err(_) => detect_episode_explained(name),
    };
    let episode = detected.episode?;
    let season = detected.season.unwrap_or(default_season).max(1);
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
pub fn detect_episode_explained(name: &str) -> EpisodeDetection {
    static MULTI_EPISODE: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
        vec![
            (
                "season_episode_range",
                hardcoded_regex(
                    r"(?i)S(?P<season>\d{1,2})[._\-\s]*E(?P<start>\d{1,4})[._\-\s]+(?:E)?(?P<end>\d{1,4})",
                ),
            ),
            (
                "episode_range",
                hardcoded_regex(
                    r"(?i)(?:^|[^\p{L}\d])E(?:P)?[._\-\s]*(?P<start>\d{1,4})[._\-\s]+(?:E(?:P)?)?(?P<end>\d{1,4})",
                ),
            ),
            (
                "collection_range",
                hardcoded_regex(
                    r"第?\s*(?P<start>\d{1,4})\s*[-~～至到]\s*(?P<end>\d{1,4})\s*[集话話]?(?:合集)?",
                ),
            ),
        ]
    });
    for (method, regex) in MULTI_EPISODE.iter() {
        if let Some(caps) = regex.captures(name) {
            let start = caps
                .name("start")
                .and_then(|m| m.as_str().parse::<i32>().ok());
            let end = caps
                .name("end")
                .and_then(|m| m.as_str().parse::<i32>().ok());
            if let (Some(start), Some(end)) = (start, end) {
                if start > 0 && end >= start && end - start <= 100 {
                    // 「Show.2023-2024」这类年份区间不是多集合集；
                    // collection_range 没有集数后缀强制，这里显式排除。
                    if *method == "collection_range"
                        && is_plausible_year(start)
                        && is_plausible_year(end)
                    {
                        continue;
                    }
                    let season = caps
                        .name("season")
                        .and_then(|m| m.as_str().parse::<i32>().ok())
                        .filter(|v| *v > 0);
                    let episodes = (start..=end).collect::<Vec<_>>();
                    return EpisodeDetection {
                        episode: Some(start),
                        episodes,
                        season,
                        special_kind: None,
                        method,
                        confidence: "high",
                        reason: format!("通过 {method} 识别为第 {start}–{end} 集多集文件"),
                    };
                }
            }
        }
    }
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
            if pattern.reject_year && episode.is_some_and(is_plausible_year) {
                continue;
            }
            return EpisodeDetection {
                episode,
                episodes: episode.into_iter().collect(),
                season,
                special_kind: match pattern.id {
                    "special_marker" => special_kind_for(name),
                    _ => None,
                },
                method: pattern.id,
                confidence: "high",
                reason: format!(
                    "通过 {} 明确格式识别到第 {} 集",
                    pattern.id,
                    episode.unwrap_or_default()
                ),
            };
        }
    }
    if let Some(episode) = numeric_fallback_episode(name) {
        return EpisodeDetection {
            episode: Some(episode),
            episodes: vec![episode],
            season: None,
            special_kind: None,
            method: "numeric_fallback",
            confidence: "low",
            reason: format!("未匹配明确标记，使用独立数字兜底识别为第 {episode} 集"),
        };
    }
    EpisodeDetection {
        episode: None,
        episodes: vec![],
        season: None,
        special_kind: None,
        method: "unrecognized",
        confidence: "none",
        reason: "未找到有效的季度或集数标记".to_string(),
    }
}

/// 进程级正则缓存。规则正则（match_regex / episode_regex / rename_regex 等）
/// 在检查、转存、预览路径中被逐文件求值，缓存编译结果避免每个文件重新编译一次。
/// 仅缓存编译成功的模式；无效模式由调用方按错误处理（本就少见且会尽快被用户修正）。
/// 放在 episode 模块是为了让 `#[path]` 内嵌本文件的集成测试无需额外 crate 根 shim。
pub fn cached_regex(pattern: &str) -> Result<std::sync::Arc<Regex>, String> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Regex>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(re) = guard.get(pattern) {
        return Ok(re.clone());
    }
    let re = Arc::new(Regex::new(pattern).map_err(|error| error.to_string())?);
    // 容量上界防止病态输入无限增长；到达后整体清空重建即可（缓存仅是性能优化）
    if guard.len() >= 256 {
        guard.clear();
    }
    guard.insert(pattern.to_string(), re.clone());
    Ok(re)
}

/// Apply a subscription-level override. The regex must expose a named `episode`
/// capture and may expose `season`; invalid/non-matching overrides safely fall back.
pub fn detect_episode_with_override(
    name: &str,
    override_regex: &str,
) -> Result<EpisodeDetection, String> {
    let pattern = override_regex.trim();
    if pattern.is_empty() {
        return Ok(detect_episode_explained(name));
    }
    let regex = cached_regex(pattern).map_err(|error| format!("episode_regex 无效：{error}"))?;
    if regex
        .capture_names()
        .flatten()
        .all(|capture| capture != "episode")
    {
        return Err("episode_regex 必须包含命名捕获 (?P<episode>...)".to_string());
    }
    let Some(caps) = regex.captures(name) else {
        return Ok(detect_episode_explained(name));
    };
    let episode = caps
        .name("episode")
        .and_then(|value| value.as_str().parse::<i32>().ok())
        .filter(|value| is_likely_explicit_episode_number(*value))
        .ok_or_else(|| "episode_regex 的 episode 捕获不是有效正整数".to_string())?;
    let season = caps
        .name("season")
        .and_then(|value| value.as_str().parse::<i32>().ok())
        .filter(|value| *value > 0);
    Ok(EpisodeDetection {
        episode: Some(episode),
        episodes: vec![episode],
        season,
        special_kind: None,
        method: "subscription_override",
        confidence: "high",
        reason: format!("通过订阅覆盖正则识别为第 {episode} 集"),
    })
}

/// 从文件名提取集数和季度。
pub fn detect_episode(name: &str) -> EpisodeInfo {
    let detected = detect_episode_explained(name);
    EpisodeInfo {
        episode: detected.episode,
        season: detected.season,
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
    fn test_matches_subscription_season_range_accepts_multi_season() {
        assert!(matches_subscription_season_range(
            "Show.S02E01.mkv",
            "Season 2",
            1,
            4
        ));
        assert!(!matches_subscription_season_range(
            "Show.S05E01.mkv",
            "Season 5",
            1,
            4
        ));
        // 无季号提示时多季也接受，转存会回落到起始季
        assert!(matches_subscription_season_range("178重置版.mp4", "", 1, 4));
        assert_eq!(resolve_file_season("Show.S03E02.mkv", "", 1, true), Some(3));
        assert_eq!(resolve_file_season("178.mp4", "", 2, false), Some(2));
        assert_eq!(resolve_file_season("178.mp4", "", 1, true), Some(1));
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

#[cfg(test)]
mod episode_corpus_tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Fixture {
        name: String,
        season: Option<i32>,
        episode: Option<i32>,
        method: String,
    }

    #[test]
    fn authoritative_episode_name_corpus_stays_compatible() {
        let fixtures: Vec<Fixture> =
            serde_json::from_str(include_str!("../../tests/fixtures/episode_names.json")).unwrap();
        for fixture in fixtures {
            let detected = detect_episode_explained(&fixture.name);
            assert_eq!(detected.season, fixture.season, "season: {}", fixture.name);
            assert_eq!(
                detected.episode, fixture.episode,
                "episode: {}",
                fixture.name
            );
            assert_eq!(detected.method, fixture.method, "method: {}", fixture.name);
            assert!(!detected.reason.is_empty());
        }
    }
}

#[cfg(test)]
mod episode_override_tests {
    use super::*;

    #[test]
    fn subscription_override_is_safe_and_explainable() {
        let found =
            detect_episode_with_override("show-2x17.mkv", r"(?P<season>\d+)x(?P<episode>\d+)")
                .unwrap();
        assert_eq!((found.season, found.episode), (Some(2), Some(17)));
        assert_eq!(found.method, "subscription_override");
        assert!(detect_episode_with_override("show.mkv", "(").is_err());
        assert!(detect_episode_with_override("show-17.mkv", r"(\d+)").is_err());
    }
}

#[cfg(test)]
mod episode_misdetection_tests {
    use super::*;

    #[test]
    fn bracketed_years_are_not_episodes() {
        // 回归：[SubGroup][2024][05] 的年份曾被识别为集数 2024，
        // 导致 known_episodes 写入 2024 并让换源/完结判定永久失效。
        let detected = detect_episode_explained("[SubGroup][2024][05][1080p].mkv");
        assert_eq!(detected.episode, Some(5));
        let detected = detect_episode_explained("Show.[2024].mkv");
        assert_eq!(detected.episode, None);
    }

    #[test]
    fn codec_and_channel_numbers_are_not_episodes() {
        let detected = detect_episode_explained("Movie.2024.H.265.mkv");
        assert_eq!(detected.episode, None);
        let detected = detect_episode_explained("Movie.2023.DD5.1.mkv");
        assert_eq!(detected.episode, None);
    }

    #[test]
    fn year_ranges_are_not_collections() {
        let detected = detect_episode_explained("Show.2023-2024.mkv");
        assert_eq!(detected.episode, None);
        // 真实集数区间不受影响
        let detected = detect_episode_explained("Show.E01-E12.mkv");
        assert_eq!(detected.episode, Some(1));
        assert_eq!(detected.episodes, (1..=12).collect::<Vec<i32>>());
    }

    #[test]
    fn pipeline_state_key_excludes_specials() {
        // 特典不占用正片集数槽位：SP01 与 EP01 的状态键不同。
        assert!(is_special_episode_name("Show SP01.mkv"));
        assert!(is_special_episode_name("动画 OVA02 BDRip.mkv"));
        assert!(!is_special_episode_name("Show S01E01.mkv"));
        assert_eq!(episode_state_key("Show SP01.mkv", 1), None);
        assert_eq!(episode_state_key("Show S01E01.mkv", 1), Some((1, 1)));
        // 展示识别保持原有语义（特典仍可解释出编号供人工查看）
        let detected = detect_episode_explained("Show SP01.mkv");
        assert_eq!(detected.episode, Some(1));
        assert_eq!(detected.special_kind, Some("sp"));
    }

    #[test]
    fn state_key_honors_subscription_override() {
        let key = episode_state_key_with_override(
            "show-2x17.mkv",
            1,
            r"(?P<season>\d+)x(?P<episode>\d+)",
        );
        assert_eq!(key, Some((2, 17)));
        // 无效 override 安全回落默认识别
        let key = episode_state_key_with_override("Show S01E17.mkv", 1, "(");
        assert_eq!(key, Some((1, 17)));
    }
}
