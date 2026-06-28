#![allow(dead_code, unused_imports)]

#[path = "../src/models/mod.rs"]
mod models;

#[path = "../src/services/episode.rs"]
pub mod episode;

mod services {
    pub use crate::episode;
}

#[path = "../src/services/transfer_rule.rs"]
mod transfer_rule;

use models::{Subscription, TransferRules};
use transfer_rule::{apply_rename, build_transfer_plan, ProbeFile};

fn subscription_with_rules(rules: TransferRules) -> Subscription {
    let mut sub: Subscription = serde_json::from_value(serde_json::json!({
        "id": "sub-flow",
        "title": "Joy of Life",
        "media_type": "series",
        "season": 1,
        "url": "https://pan.quark.cn/s/test",
        "created_at": 1,
        "updated_at": 1,
        "last_checked_at": 1,
        "rules": rules
    }))
    .expect("test subscription should deserialize");
    sub.rules.exclude_keywords.clear();
    sub
}

fn probe_file(name: &str, fid: &str, parent_path: &str, size: i64) -> ProbeFile {
    ProbeFile {
        name: name.to_string(),
        fid: fid.to_string(),
        is_dir: false,
        size,
        parent_path: parent_path.to_string(),
        updated_at: None,
    }
}

#[test]
fn rename_template_application_keeps_episode_and_extension() {
    let rules = TransferRules {
        rename_template: "{title}.S{season}E{episode}.{ext}".to_string(),
        ..Default::default()
    };
    let sub = subscription_with_rules(rules.clone());

    let (target, error) = apply_rename("第05集.mkv", &rules, Some(&sub), Some(5));

    assert!(error.is_none());
    assert_eq!(target, "Joy of Life.S01E05.mkv");
}

#[test]
fn duplicate_episode_dedup_keeps_highest_quality_candidate() {
    let rules = TransferRules {
        duplicate_episode_strategy: "highest_quality".to_string(),
        ..Default::default()
    };
    let sub = subscription_with_rules(rules);
    let files = vec![
        probe_file("Joy.of.Life.S01E01.720p.mkv", "fid-720", "", 700),
        probe_file("Joy.of.Life.S01E01.1080p.mkv", "fid-1080", "", 900),
        probe_file("Joy.of.Life.S01E02.720p.mkv", "fid-2", "", 700),
    ];

    let plan = build_transfer_plan(&sub, Some(&files), None, None, Some(true));

    assert_eq!(plan.transfer_count, 2);
    assert!(plan
        .transfers
        .iter()
        .any(|item| item.source_name == "Joy.of.Life.S01E01.1080p.mkv"));
    assert!(plan.skipped.iter().any(|item| {
        item.source_name == "Joy.of.Life.S01E01.720p.mkv"
            && item.skip_reason.contains("同集重复视频")
    }));
}

#[test]
fn season_and_start_episode_filters_are_applied_together() {
    let rules = TransferRules::default();
    let mut sub = subscription_with_rules(rules);
    sub.season = 2;
    sub.start_episode_number = Some(5);
    let files = vec![
        probe_file("Joy.of.Life.S01E05.mkv", "fid-s1", "", 900),
        probe_file("Joy.of.Life.S02E04.mkv", "fid-old", "", 900),
        probe_file("Joy.of.Life.S02E05.mkv", "fid-new", "", 900),
    ];

    let plan = build_transfer_plan(&sub, Some(&files), None, None, Some(true));

    assert_eq!(plan.transfer_count, 1);
    assert_eq!(plan.transfers[0].source_name, "Joy.of.Life.S02E05.mkv");
    assert!(plan.skipped.iter().any(|item| {
        item.source_name == "Joy.of.Life.S01E05.mkv" && item.skip_reason == "非当前订阅季"
    }));
    assert!(plan.skipped.iter().any(|item| {
        item.source_name == "Joy.of.Life.S02E04.mkv"
            && item.skip_reason == "低于起始转存集数：第 5 集"
    }));
}
