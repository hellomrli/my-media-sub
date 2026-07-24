use std::collections::BTreeSet;
use std::sync::LazyLock;

use chrono::{DateTime, TimeZone, Utc};
use regex::Regex;

use crate::models::SourceQuality;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts", "m2ts", "rmvb", "iso",
];

#[derive(Debug, Clone, Default)]
pub struct SourceQualityFile {
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    pub updated_at: Option<String>,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SourceQualityInput {
    pub title: String,
    pub datetime: String,
    pub validity: Option<bool>,
    pub probe_ok: Option<bool>,
    pub probe_file_count: usize,
    pub probe_episode_count: usize,
    pub files: Vec<SourceQualityFile>,
}

static EPISODE_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"(?i)S\d{1,2}E(\d{1,4})").expect("valid source quality episode regex"),
        Regex::new(r"(?i)(?:^|[\s._\-\[【(])EP?\s*(\d{1,4})(?:$|[\s._\-\]】)])")
            .expect("valid source quality episode regex"),
        Regex::new(r"第\s*(\d{1,4})\s*[集话]").expect("valid source quality episode regex"),
    ]
});

pub fn score_source(input: &SourceQualityInput, now_ms: i64) -> SourceQuality {
    let regular_files = input
        .files
        .iter()
        .filter(|file| !file.is_dir)
        .collect::<Vec<_>>();
    let video_files = regular_files
        .iter()
        .copied()
        .filter(|file| is_video_file(file))
        .collect::<Vec<_>>();
    let searchable_text = std::iter::once(input.title.as_str())
        .chain(video_files.iter().take(30).map(|file| file.name.as_str()))
        .collect::<Vec<_>>()
        .join(" ");
    let validity = input.validity.or(input.probe_ok);
    let (resolution, resolution_score) = resolution_info(&searchable_text);
    let file_count = input.probe_file_count.max(input.files.len());
    let video_count = video_files.len();
    let episodes = infer_episodes(&video_files);
    let episode_count = input.probe_episode_count.max(episodes.len());
    let total_size = regular_files
        .iter()
        .map(|file| file.size.max(0))
        .sum::<i64>();

    let updated_ms = std::iter::once(parse_timestamp_ms(&input.datetime))
        .chain(
            input
                .files
                .iter()
                .map(|file| parse_timestamp_ms(file.updated_at.as_deref().unwrap_or_default())),
        )
        .flatten()
        .max();
    let age_days = updated_ms
        .map(|timestamp| (now_ms.saturating_sub(timestamp)).max(0) as f64 / 86_400_000.0)
        .unwrap_or(f64::INFINITY);

    let mut tags = Vec::new();
    push_unique(&mut tags, (!resolution.is_empty()).then_some(resolution));
    if contains(&searchable_text, r"(?i)\b(?:dolby[ ._-]?vision|dovi|dv)\b") {
        push_unique(&mut tags, Some("杜比视界"));
    } else if contains(&searchable_text, r"(?i)\b(?:hdr10\+?|hdr)\b") {
        push_unique(&mut tags, Some("HDR"));
    }
    if contains(&searchable_text, r"(?i)\bav1\b") {
        push_unique(&mut tags, Some("AV1"));
    } else if contains(&searchable_text, r"(?i)\b(?:x265|h\.?265|hevc)\b") {
        push_unique(&mut tags, Some("H.265"));
    } else if contains(&searchable_text, r"(?i)\b(?:x264|h\.?264|avc)\b") {
        push_unique(&mut tags, Some("H.264"));
    }
    if contains(&searchable_text, r"(?i)\b(?:atmos|truehd|dts[ ._-]?hd)\b") {
        push_unique(&mut tags, Some("高规格音轨"));
    }
    if contains(&searchable_text, r"(?i)\b(?:web[ ._-]?dl|webrip)\b") {
        push_unique(&mut tags, Some("WEB"));
    } else if contains(&searchable_text, r"(?i)\b(?:blu[ ._-]?ray|bdrip|remux)\b") {
        push_unique(&mut tags, Some("蓝光"));
    }
    if episode_count > 0 {
        push_owned_unique(&mut tags, format!("{} 集", episode_count));
    }

    let mut risks = Vec::new();
    if validity == Some(false) {
        push_unique(&mut risks, Some("链接已失效"));
    }
    if contains(&searchable_text, r"广告|推广|公众号|加群|解压密码|防失联") {
        push_unique(&mut risks, Some("疑似广告内容"));
    }
    if contains(
        &searchable_text,
        r"前\s*[一二三四五六七八九十\d]+\s*季|全\s*[一二三四五六七八九十\d]+\s*季|多季|合集|大合集",
    ) {
        push_unique(&mut risks, Some("合集或跨季风险"));
    }
    if input.probe_ok == Some(true) && file_count > 0 && video_count == 0 {
        push_unique(&mut risks, Some("未发现视频文件"));
    }

    let mut score = 24 + resolution_score;
    match validity {
        Some(true) => score += 18,
        None => score += 5,
        Some(false) => score -= 32,
    }
    if has_tag(&tags, "杜比视界") || has_tag(&tags, "HDR") {
        score += 7;
    }
    if has_tag(&tags, "AV1") || has_tag(&tags, "H.265") {
        score += 6;
    } else if has_tag(&tags, "H.264") {
        score += 3;
    }
    if has_tag(&tags, "高规格音轨") {
        score += 5;
    }
    if has_tag(&tags, "蓝光") {
        score += 5;
    } else if has_tag(&tags, "WEB") {
        score += 3;
    }
    if video_count > 0 {
        score += 7;
    }
    if episode_count > 0 {
        score += 11.min(4 + (episode_count as i32 + 5) / 6);
    }
    if file_count > 0 {
        score += 3;
    }
    if age_days <= 7.0 {
        score += 8;
    } else if age_days <= 30.0 {
        score += 5;
    } else if age_days <= 180.0 {
        score += 2;
    }
    score -= risks
        .iter()
        .filter(|risk| risk.as_str() != "链接已失效")
        .count() as i32
        * 9;
    score = score.clamp(0, 100);
    if validity == Some(false) {
        score = score.min(24);
    }

    let (grade, tone) = match score {
        85..=100 => ("旗舰", "excellent"),
        70..=84 => ("优质", "good"),
        55..=69 => ("清晰", "fair"),
        35..=54 => ("普通", "muted"),
        _ => ("谨慎", "danger"),
    };
    let updated_at = updated_ms.and_then(|timestamp| {
        Utc.timestamp_millis_opt(timestamp)
            .single()
            .map(|date| date.to_rfc3339())
    });
    let episode_start = episodes.iter().next().copied();
    let episode_end = episodes.iter().next_back().copied();
    let recommendation_reasons = recommendation_reasons(
        score as u8,
        validity,
        resolution,
        episode_count,
        age_days,
        &risks,
    );

    SourceQuality {
        score: score as u8,
        grade: grade.to_string(),
        tone: tone.to_string(),
        tags: tags.into_iter().take(6).collect(),
        risks,
        resolution: if resolution.is_empty() {
            "未知清晰度".to_string()
        } else {
            resolution.to_string()
        },
        file_count,
        video_count,
        episode_count,
        episode_start,
        episode_end,
        total_size,
        updated_at,
        recommendation_reasons,
    }
}

fn is_video_file(file: &SourceQualityFile) -> bool {
    if file.is_dir {
        return false;
    }
    if file
        .category
        .as_deref()
        .is_some_and(|category| category.to_ascii_lowercase().contains("video"))
    {
        return true;
    }
    let extension = file
        .name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    VIDEO_EXTENSIONS.contains(&extension.as_str())
}

fn infer_episodes(files: &[&SourceQualityFile]) -> BTreeSet<i32> {
    let mut episodes = BTreeSet::new();
    for file in files {
        for pattern in EPISODE_PATTERNS.iter() {
            let matched = pattern
                .captures_iter(&file.name)
                .filter_map(|capture| capture.get(1))
                .filter_map(|value| value.as_str().parse::<i32>().ok())
                .filter(|episode| *episode > 0)
                .collect::<Vec<_>>();
            if !matched.is_empty() {
                episodes.extend(matched);
                break;
            }
        }
    }
    episodes
}

fn resolution_info(text: &str) -> (&'static str, i32) {
    if contains(text, r"(?i)\b(?:8k|4320p)\b") {
        ("8K", 32)
    } else if contains(text, r"(?i)\b(?:4k|uhd|2160p)\b") {
        ("4K", 28)
    } else if contains(text, r"(?i)\b(?:1440p|2k)\b") {
        ("2K", 23)
    } else if contains(text, r"(?i)\b(?:1080p?|fhd)\b") {
        ("1080P", 19)
    } else if contains(text, r"(?i)\b(?:720p?|hd)\b") {
        ("720P", 11)
    } else if contains(text, r"(?i)\b(?:480p?|sd)\b") {
        ("SD", 4)
    } else {
        ("", 0)
    }
}

fn contains(text: &str, pattern: &str) -> bool {
    crate::services::episode::cached_regex(pattern)
        .expect("valid source quality regex")
        .is_match(text)
}

fn parse_timestamp_ms(value: &str) -> Option<i64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(number) = value.parse::<f64>() {
        if number.is_finite() && number != 0.0 {
            return Some(if number.abs() < 1_000_000_000_000.0 {
                (number * 1000.0) as i64
            } else {
                number as i64
            });
        }
    }
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp_millis())
}

fn has_tag(tags: &[String], expected: &str) -> bool {
    tags.iter().any(|tag| tag == expected)
}

fn push_unique(items: &mut Vec<String>, value: Option<&str>) {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return;
    };
    if !items.iter().any(|item| item == value) {
        items.push(value.to_string());
    }
}

fn push_owned_unique(items: &mut Vec<String>, value: String) {
    if !value.is_empty() && !items.contains(&value) {
        items.push(value);
    }
}

fn recommendation_reasons(
    score: u8,
    validity: Option<bool>,
    resolution: &str,
    episode_count: usize,
    age_days: f64,
    risks: &[String],
) -> Vec<String> {
    let mut reasons = Vec::new();
    if score >= 85 {
        reasons.push("综合质量优秀".to_string());
    } else if score >= 70 {
        reasons.push("综合质量较高".to_string());
    }
    if validity == Some(true) {
        reasons.push("分享链接探测有效".to_string());
    }
    if !resolution.is_empty() {
        reasons.push(format!("清晰度识别为 {}", resolution));
    }
    if episode_count > 0 {
        reasons.push(format!("识别到 {} 集视频", episode_count));
    }
    if age_days <= 7.0 {
        reasons.push("最近 7 天有更新".to_string());
    }
    if risks.is_empty() {
        reasons.push("未发现明显风险".to_string());
    }
    reasons.truncate(5);
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Fixture {
        name: String,
        now_ms: i64,
        input: FixtureInput,
        expected: FixtureExpected,
    }

    #[derive(Deserialize)]
    struct FixtureInput {
        title: String,
        datetime: String,
        validity: Option<bool>,
        probe_ok: Option<bool>,
        probe_file_count: usize,
        probe_episode_count: usize,
        files: Vec<FixtureFile>,
    }

    #[derive(Deserialize)]
    struct FixtureFile {
        name: String,
        #[serde(default)]
        is_dir: bool,
        #[serde(default)]
        size: i64,
        updated_at: Option<String>,
        category: Option<String>,
    }

    #[derive(Deserialize)]
    struct FixtureExpected {
        score: u8,
        grade: String,
        resolution: String,
        video_count: usize,
        episode_count: usize,
        risks: Vec<String>,
    }

    #[test]
    fn shared_quality_fixtures_match_authoritative_scoring() {
        let fixtures: Vec<Fixture> =
            serde_json::from_str(include_str!("../../tests/fixtures/source_quality.json")).unwrap();
        for fixture in fixtures {
            let input = SourceQualityInput {
                title: fixture.input.title,
                datetime: fixture.input.datetime,
                validity: fixture.input.validity,
                probe_ok: fixture.input.probe_ok,
                probe_file_count: fixture.input.probe_file_count,
                probe_episode_count: fixture.input.probe_episode_count,
                files: fixture
                    .input
                    .files
                    .into_iter()
                    .map(|file| SourceQualityFile {
                        name: file.name,
                        is_dir: file.is_dir,
                        size: file.size,
                        updated_at: file.updated_at,
                        category: file.category,
                    })
                    .collect(),
            };
            let quality = score_source(&input, fixture.now_ms);
            assert_eq!(quality.score, fixture.expected.score, "{}", fixture.name);
            assert_eq!(quality.grade, fixture.expected.grade, "{}", fixture.name);
            assert_eq!(
                quality.resolution, fixture.expected.resolution,
                "{}",
                fixture.name
            );
            assert_eq!(
                quality.video_count, fixture.expected.video_count,
                "{}",
                fixture.name
            );
            assert_eq!(
                quality.episode_count, fixture.expected.episode_count,
                "{}",
                fixture.name
            );
            assert_eq!(quality.risks, fixture.expected.risks, "{}", fixture.name);
        }
    }
}
