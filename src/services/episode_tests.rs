//! `episode` 的单元测试（含语料驱动的识别回归）。
//!
//! 原本内联在母文件里（393 行），外移后母文件只保留生产代码。
//! 注意：`episode.rs` 用显式 `#[path = "episode_tests.rs"]` 声明本模块，因为
//! `tests/subscription_flow.rs` 会以 `#[path]` 直接编译 episode.rs，那种上下文里
//! 相对的 `mod tests;` 会解析到不存在的路径。
//!
//! 缩进层级：本文件即 `episode::tests`。文件顶层的项需要 `use super::*` 指向
//! `episode`；而下面各 `mod *_tests` 子模块比 `episode` 低两层，需要
//! `use super::super::*`。

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

#[cfg(test)]
mod episode_corpus_tests {
    use super::super::*;
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
    use super::super::*;

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
    use super::super::*;

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
    fn explicit_marker_years_are_not_episodes() {
        // 回归：年份排除原先只加在 bracket_number 上，其余明确模式照单全收，
        // `OVA 2024` 因此被识别成第 2024 集——未设总集数的订阅会把它当正片
        // 转存并按 {episode} 重命名，设了总集数则被"集数超过订阅总集数"跳过
        // 并给出误导性原因（真实原因是识别错了集数）。
        for name in [
            "动画 OVA 2024 [1080p].mkv",
            "[Group] Show SP 2023 [BDRip].mkv",
            "某剧 OAD 2022.mkv",
            "Show S01E1998.mkv",
            "Show.E2024.1080p.mkv",
            "某剧 第2024集.mkv",
        ] {
            assert_eq!(
                detect_episode_explained(name).episode,
                None,
                "年份被当成集数: {name}"
            );
        }
    }

    #[test]
    fn year_rejection_keeps_real_specials_and_season_episodes() {
        // 年份排除不得波及真实特典编号，也不得影响"年份 + 季集"共存的常见命名。
        let detected = detect_episode_explained("动画 OVA02 BDRip.mkv");
        assert_eq!(detected.episode, Some(2));
        assert_eq!(detected.special_kind, Some("ova"));
        assert_eq!(detect_episode_explained("Show SP01.mkv").episode, Some(1));
        assert_eq!(
            detect_episode_explained("Show.2024.S01E05.1080p.mkv").episode,
            Some(5)
        );
        assert_eq!(
            detect_episode_explained("Show.2024.EP05.1080p.mkv").episode,
            Some(5)
        );
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
