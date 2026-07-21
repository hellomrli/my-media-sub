//! 分享标题清洗：剥离字幕组/画质/季标噪声，供 TMDB 匹配与订阅命名共用。

use regex::Regex;
use std::sync::LazyLock;

static SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:\s*(?:S\d{1,2}(?:\s*[-~～到至]\s*S?\d{1,2})?|Season\s*\d+(?:\s*[-~～到至]\s*\d+)?|第\s*[0-9一二三四五六七八九十两]+\s*季(?:\s*[-~～到至]\s*第?\s*[0-9一二三四五六七八九十两]+\s*季)?|\d{3,4}p|4k|8k|web-?dl|bluray|bdrip|hdtv|x26[45]|hevc|aac|flac|全\s*\d+\s*集|全集|完结|更新至.*))+$",
    )
    .expect("title suffix regex")
});

/// 清洗结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedTitle {
    pub original: String,
    pub normalized: String,
}

/// 从分享标题中剥离噪声，得到更利于元数据匹配的剧名。
pub fn clean_media_title(title: &str) -> String {
    normalize_title_detailed(title).normalized
}

pub fn normalize_title_detailed(title: &str) -> NormalizedTitle {
    let original = title.trim().to_string();
    if original.is_empty() || original.to_ascii_lowercase().starts_with("http") {
        return NormalizedTitle {
            original: original.clone(),
            normalized: original,
        };
    }

    let mut output = String::new();
    let mut bracket_depth = 0usize;
    for ch in original.chars() {
        match ch {
            '[' | '【' | '(' | '（' => bracket_depth += 1,
            ']' | '】' | ')' | '）' => bracket_depth = bracket_depth.saturating_sub(1),
            '.' | '_' | '-' => {
                if bracket_depth == 0 {
                    output.push(' ');
                }
            }
            _ if bracket_depth == 0 => output.push(ch),
            _ => {}
        }
    }

    let mut cleaned = collapse_spaces(&output);
    cleaned = trim_bilingual_prefix(&cleaned);
    cleaned = SUFFIX_RE.replace(&cleaned, "").to_string();
    cleaned = collapse_spaces(&cleaned);

    let normalized = if cleaned.is_empty() {
        original.clone()
    } else {
        cleaned
    };

    NormalizedTitle {
        original,
        normalized,
    }
}

fn collapse_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 中日/中英并列标题时优先保留中文段。
fn trim_bilingual_prefix(value: &str) -> String {
    let title = value.trim();
    if title.is_empty() {
        return String::new();
    }

    if let Some(kana_index) = title.find(|ch: char| ('\u{3040}'..='\u{30ff}').contains(&ch)) {
        if kana_index > 0
            && title[..kana_index]
                .chars()
                .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch))
        {
            return title[..kana_index]
                .trim_end_matches(|ch: char| {
                    matches!(
                        ch,
                        ' ' | '·'
                            | '・'
                            | ','
                            | '，'
                            | '/'
                            | '|'
                            | ':'
                            | '：'
                            | '-'
                            | '–'
                            | '—'
                            | '_'
                    )
                })
                .to_string();
        }
    }

    let parts: Vec<&str> = title
        .split(['|', '/', '／'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() > 1
        && parts[0]
            .chars()
            .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch))
    {
        return parts[0].to_string();
    }

    title.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_fansub_and_quality_noise() {
        assert_eq!(
            clean_media_title("【字幕组】庆余年 1080p S01-S04 全集"),
            "庆余年"
        );
        assert_eq!(clean_media_title("庆余年（2024）[简中]"), "庆余年");
        assert_eq!(
            clean_media_title("孤独摇滚！ / Bocchi the Rock!"),
            "孤独摇滚！"
        );
    }

    #[test]
    fn keeps_urls_untouched() {
        let url = "https://pan.quark.cn/s/abc";
        assert_eq!(clean_media_title(url), url);
    }
}
