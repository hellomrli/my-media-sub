#![allow(dead_code)]

use crate::models::{Subscription, TransferRules};
use crate::services::episode::{detect_episode, split_words};
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;

/// 转存计划
#[derive(Debug, Clone)]
pub struct TransferPlan {
    pub target_dir: String,
    pub target_dir_exists: Option<bool>,
    pub auto_create_target_dir: bool,
    pub items: Vec<TransferItem>,
    pub transfers: Vec<TransferItem>,
    pub skipped: Vec<TransferItem>,
    pub transfer_count: usize,
    pub skip_count: usize,
    pub matched_count: usize,
    pub episodes: Vec<i32>,
    pub current_episode_number: i32,
    pub summary: String,
}

/// 转存项目
#[derive(Debug, Clone)]
pub struct TransferItem {
    pub source_name: String,
    pub source_fid: String,
    pub episode: Option<i32>,
    pub season: Option<i32>,
    pub file_key: String,
    pub target_dir: String,
    pub target_name: String,
    pub action: String, // "transfer" 或 "skip"
    pub skip_reason: String,
}

/// 探测文件（简化结构）
#[derive(Debug, Clone)]
pub struct ProbeFile {
    pub name: String,
    pub fid: String,
    pub is_dir: bool,
}

/// 规范化规则（填充默认值）
pub fn normalize_rules(rules: Option<&TransferRules>) -> TransferRules {
    rules.cloned().unwrap_or_default()
}

/// 显示名称（可选忽略扩展名）
fn display_name(name: &str, ignore_extensions: bool) -> String {
    if !ignore_extensions {
        return name.to_string();
    }
    let suffix = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if suffix.is_empty() {
        name.to_string()
    } else {
        name.strip_suffix(&format!(".{}", suffix))
            .unwrap_or(name)
            .to_string()
    }
}

/// 状态键（用于去重）
fn state_key(name: &str, episode: Option<i32>, ignore_extensions: bool) -> String {
    let comparable = display_name(name, ignore_extensions).to_lowercase();
    if let Some(ep) = episode {
        format!("ep:{}:{}", ep, comparable)
    } else {
        format!("name:{}", comparable)
    }
}

/// 检查是否包含关键词
fn has_words(name: &str, words: &[String]) -> bool {
    let lower = name.to_lowercase();
    words.iter().any(|w| lower.contains(&w.to_lowercase()))
}

/// 格式化集数
fn format_episode(episode: Option<i32>) -> String {
    match episode {
        Some(ep) if ep < 100 => format!("{:02}", ep),
        Some(ep) => ep.to_string(),
        None => String::new(),
    }
}

fn format_season(season: i32) -> String {
    if season <= 0 {
        String::new()
    } else if season < 100 {
        format!("{:02}", season)
    } else {
        season.to_string()
    }
}

/// 应用重命名规则
pub fn apply_rename(
    name: &str,
    rules: &TransferRules,
    subscription: Option<&Subscription>,
    episode: Option<i32>,
) -> (String, Option<String>) {
    let ignore_ext = rules.ignore_extensions;
    let suffix = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{}", s))
        .unwrap_or_default();
    let base = display_name(name, ignore_ext);
    let rename_input = if ignore_ext { &base } else { name };
    let mut target = rename_input.to_string();

    // 正则替换
    if !rules.rename_regex.is_empty() {
        match Regex::new(&rules.rename_regex) {
            Ok(re) => {
                target = re
                    .replace_all(&target, &rules.rename_replacement)
                    .to_string();
            }
            Err(e) => return (name.to_string(), Some(format!("rename_regex 无效：{}", e))),
        }
    }

    // 模板替换
    if !rules.rename_template.is_empty() {
        let template = &rules.rename_template;
        let title = subscription.map(|s| s.title.as_str()).unwrap_or("");
        let season = subscription.map(|s| s.season).unwrap_or(0);
        let season_str = format_season(season);
        let season_number = if season > 0 {
            season.to_string()
        } else {
            String::new()
        };
        let episode_str = format_episode(episode);
        let original = display_name(name, true);
        let name_part = display_name(&target, true);
        let ext = suffix.trim_start_matches('.');

        target = template
            .replace("{}", &episode_str)
            .replace("{title}", title)
            .replace("{season}", &season_str)
            .replace("{season_number}", &season_number)
            .replace("{episode}", &episode_str)
            .replace(
                "{episode_number}",
                &episode.map(|e| e.to_string()).unwrap_or_default(),
            )
            .replace("{original}", &original)
            .replace("{name}", &name_part)
            .replace("{ext}", ext);
    }

    // 补充扩展名
    let known_media_suffixes = [
        ".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv", ".ts", ".m2ts", ".webm", ".srt", ".ass",
        ".ssa",
    ];
    if !suffix.is_empty()
        && !known_media_suffixes
            .iter()
            .any(|ext| target.to_lowercase().ends_with(ext))
    {
        target = format!("{}{}", target, suffix);
    }

    (
        if target.is_empty() {
            name.to_string()
        } else {
            target
        },
        None,
    )
}

/// 构建转存计划
pub fn build_transfer_plan(
    subscription: &Subscription,
    probe_files: Option<&[ProbeFile]>,
    transferred_keys: Option<&HashSet<String>>,
    target_existing_files: Option<&[String]>,
    target_dir_exists: Option<bool>,
) -> TransferPlan {
    let rules = normalize_rules(Some(&subscription.rules));
    let files: Vec<ProbeFile> = probe_files.map(|f| f.to_vec()).unwrap_or_default();
    let transferred = transferred_keys
        .cloned()
        .unwrap_or_else(|| subscription.transferred_file_keys.iter().cloned().collect());
    let existing: HashSet<String> = target_existing_files
        .map(|e| {
            e.iter()
                .map(|name| display_name(name, rules.ignore_extensions).to_lowercase())
                .collect()
        })
        .unwrap_or_default();
    let target_dir = if rules.target_dir.is_empty() {
        format!("/{}", subscription.title)
    } else {
        rules.target_dir.clone()
    };

    let include_kw = split_words(&rules.include_keywords);
    let exclude_kw = split_words(&rules.exclude_keywords);

    let mut items: Vec<TransferItem> = Vec::new();
    let mut matched_for_summary: Vec<(String, Option<i32>, String)> = Vec::new();
    let compile_error: Option<String> = if !rules.match_regex.is_empty() {
        Regex::new(&rules.match_regex).err().map(|e| e.to_string())
    } else {
        None
    };
    let match_re = if compile_error.is_none() && !rules.match_regex.is_empty() {
        Regex::new(&rules.match_regex).ok()
    } else {
        None
    };

    for raw in &files {
        let name = &raw.name;
        let ep_info = detect_episode(name);
        let episode = ep_info.episode;
        let key = state_key(name, episode, rules.ignore_extensions);
        let comparable = display_name(name, rules.ignore_extensions);

        let mut item = TransferItem {
            source_name: name.clone(),
            source_fid: raw.fid.clone(),
            episode,
            season: ep_info.season,
            file_key: key.clone(),
            target_dir: target_dir.clone(),
            target_name: name.clone(),
            action: "skip".to_string(),
            skip_reason: String::new(),
        };

        // 过滤逻辑
        if raw.is_dir {
            item.skip_reason = "目录暂不规划转存".to_string();
        } else if name.is_empty() {
            item.skip_reason = "文件名为空".to_string();
        } else if !include_kw.is_empty() && !has_words(&comparable, &include_kw) {
            item.skip_reason = "不含包含关键词".to_string();
        } else if !exclude_kw.is_empty() && has_words(&comparable, &exclude_kw) {
            item.skip_reason = "命中排除关键词".to_string();
        } else if let Some(err) = &compile_error {
            item.skip_reason = format!("match_regex 无效：{}", err);
        } else if let Some(re) = &match_re {
            if !re.is_match(&comparable) {
                item.skip_reason = "未命中匹配正则".to_string();
            }
        }

        if item.skip_reason.is_empty() && subscription.media_type != "movie" {
            if let Some(start_episode) = subscription.start_episode_number {
                if start_episode > 1
                    && episode
                        .map(|episode| episode < start_episode)
                        .unwrap_or(false)
                {
                    item.skip_reason = format!("低于起始转存集数：第 {} 集", start_episode);
                }
            }
        }

        // 通过过滤，检查转存条件
        if item.skip_reason.is_empty() {
            if rules.skip_existing_transferred && transferred.contains(&key) {
                item.skip_reason = "已转存记录中存在".to_string();
            } else {
                let (target_name, rename_error) =
                    apply_rename(name, &rules, Some(subscription), episode);
                item.target_name = target_name.clone();
                let target_compare =
                    display_name(&target_name, rules.ignore_extensions).to_lowercase();

                if let Some(err) = rename_error {
                    item.skip_reason = err;
                } else if existing.contains(&target_compare) {
                    item.skip_reason = "目标目录已有同名文件".to_string();
                } else if target_dir_exists == Some(false) && !rules.auto_create_target_dir {
                    item.skip_reason = "目标目录不存在且未开启自动新建".to_string();
                } else {
                    item.action = "transfer".to_string();
                    matched_for_summary.push((name.clone(), episode, key.clone()));
                }
            }
        }

        items.push(item);
    }

    // only_latest 逻辑
    if rules.only_latest {
        let transfer_items: Vec<_> = items.iter().filter(|i| i.action == "transfer").collect();
        let episodes: Vec<i32> = transfer_items.iter().filter_map(|i| i.episode).collect();
        if let Some(&latest) = episodes.iter().max() {
            matched_for_summary.clear();
            for item in &mut items {
                if item.action == "transfer" && item.episode != Some(latest) {
                    item.action = "skip".to_string();
                    item.skip_reason = "only_latest 仅处理最新一集".to_string();
                }
                if item.action == "transfer" {
                    matched_for_summary.push((
                        item.source_name.clone(),
                        item.episode,
                        item.file_key.clone(),
                    ));
                }
            }
        }
    }

    let transfers: Vec<_> = items
        .iter()
        .filter(|i| i.action == "transfer")
        .cloned()
        .collect();
    let skipped: Vec<_> = items
        .iter()
        .filter(|i| i.action == "skip")
        .cloned()
        .collect();

    // 汇总集数
    let normalized_matched: Vec<_> = items
        .iter()
        .filter(|i| {
            !i.skip_reason.starts_with("match_regex 无效")
                && i.skip_reason != "目录暂不规划转存"
                && i.skip_reason != "文件名为空"
                && i.skip_reason != "不含包含关键词"
                && i.skip_reason != "命中排除关键词"
                && i.skip_reason != "未命中匹配正则"
        })
        .collect();
    let episodes: Vec<i32> = normalized_matched
        .iter()
        .filter_map(|i| i.episode)
        .collect();
    let mut unique_episodes: Vec<i32> = episodes
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    unique_episodes.sort_unstable();
    let current_episode_number = unique_episodes.iter().max().copied().unwrap_or(0);

    TransferPlan {
        target_dir: target_dir.clone(),
        target_dir_exists,
        auto_create_target_dir: rules.auto_create_target_dir,
        items: items.clone(),
        transfers: transfers.clone(),
        skipped: skipped.clone(),
        transfer_count: transfers.len(),
        skip_count: skipped.len(),
        matched_count: normalized_matched.len(),
        episodes: unique_episodes,
        current_episode_number,
        summary: format!(
            "规划转存 {} 个，跳过 {} 个，目标目录：{}",
            transfers.len(),
            skipped.len(),
            target_dir
        ),
    }
}

/// 规则摘要
pub fn summarize_rules(rules: Option<&TransferRules>) -> String {
    let rules = normalize_rules(rules);
    let mut parts = Vec::new();

    if !rules.target_dir.is_empty() {
        parts.push(format!("目录 {}", rules.target_dir));
    }
    if !rules.match_regex.is_empty() {
        parts.push(format!("正则 {}", rules.match_regex));
    }
    let include_kw = split_words(&rules.include_keywords);
    if !include_kw.is_empty() {
        parts.push(format!("包含 {}", include_kw.join("/")));
    }
    let exclude_kw = split_words(&rules.exclude_keywords);
    if !exclude_kw.is_empty() {
        let shown: Vec<_> = exclude_kw.iter().take(4).cloned().collect();
        parts.push(format!("排除 {}", shown.join("/")));
    }
    if !rules.rename_template.is_empty() {
        parts.push(format!("模板 {}", rules.rename_template));
    }
    if !rules.rename_regex.is_empty() {
        parts.push(format!(
            "替换 {}→{}",
            rules.rename_regex, rules.rename_replacement
        ));
    }
    if rules.only_latest {
        parts.push("仅最新".to_string());
    }
    if rules.skip_existing_transferred {
        parts.push("跳过已转存".to_string());
    }

    if parts.is_empty() {
        "默认规则".to_string()
    } else {
        parts.join("；")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sub(title: &str, rules: TransferRules) -> Subscription {
        Subscription {
            id: "test".to_string(),
            title: title.to_string(),
            source_title: String::new(),
            media_type: "series".to_string(),
            season: 1,
            start_episode_number: None,
            current_episode_number: 0,
            total_episode_number: None,
            source_group: String::new(),
            metadata: None,
            cloud_type: "quark".to_string(),
            url: "https://test".to_string(),
            password: String::new(),
            known_files: vec![],
            known_file_keys: vec![],
            known_episodes: vec![],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            last_probe: None,
            last_plan_summary: String::new(),
            notify_only: false,
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            enabled: true,
            completed: false,
            rules,
            created_at: 1,
            updated_at: 1,
            last_checked_at: 1,
            last_new_files: vec![],
            last_new_episodes: vec![],
            last_check_summary: String::new(),
            check_history: vec![],
            status: "active".to_string(),
            invalid_since: None,
            last_error: String::new(),
            rule_summary: String::new(),
        }
    }

    fn make_file(name: &str) -> ProbeFile {
        ProbeFile {
            name: name.to_string(),
            fid: "fid123".to_string(),
            is_dir: false,
        }
    }

    #[test]
    fn test_build_transfer_plan_basic() {
        let rules = TransferRules {
            target_dir: "/test".to_string(),
            ..Default::default()
        };
        let sub = make_sub("测试", rules);
        let files = vec![
            make_file("第01集.mkv"),
            make_file("第02集.mkv"),
            make_file("预告.mp4"),
        ];

        let plan = build_transfer_plan(&sub, Some(&files), None, None, None);
        assert_eq!(plan.transfer_count, 2); // 预告被排除
        assert_eq!(plan.current_episode_number, 2);
        assert_eq!(plan.episodes, vec![1, 2]);
    }

    #[test]
    fn test_build_transfer_plan_respects_start_episode_number() {
        let rules = TransferRules::default();
        let mut sub = make_sub("Show", rules);
        sub.start_episode_number = Some(5);
        let files = vec![make_file("Show.S01E04.mkv"), make_file("Show.S01E05.mkv")];

        let plan = build_transfer_plan(&sub, Some(&files), None, None, None);

        assert_eq!(plan.transfer_count, 1);
        assert_eq!(plan.transfers[0].source_name, "Show.S01E05.mkv");
        assert_eq!(plan.skipped[0].skip_reason, "低于起始转存集数：第 5 集");
    }

    #[test]
    fn test_apply_rename_template() {
        let rules = TransferRules {
            rename_template: "Show.S01E{}".to_string(),
            ..Default::default()
        };
        let (result, err) = apply_rename("第05集.mkv", &rules, None, Some(5));
        assert_eq!(result, "Show.S01E05.mkv");
        assert!(err.is_none());
    }

    #[test]
    fn test_apply_rename_template_respects_unset_season() {
        let rules = TransferRules {
            rename_template: "{title}.E{}".to_string(),
            ..Default::default()
        };
        let mut sub = make_sub("Show", rules.clone());
        sub.season = 0;

        let (result, err) = apply_rename("第05集.mkv", &rules, Some(&sub), Some(5));
        assert_eq!(result, "Show.E05.mkv");
        assert!(err.is_none());

        let rules = TransferRules {
            rename_template: "{title}.S{season}E{}".to_string(),
            ..Default::default()
        };
        sub.season = 2;
        let (result, err) = apply_rename("第05集.mkv", &rules, Some(&sub), Some(5));
        assert_eq!(result, "Show.S02E05.mkv");
        assert!(err.is_none());
    }

    #[test]
    fn test_summarize_rules() {
        let rules = TransferRules {
            target_dir: "/anime".to_string(),
            rename_template: "Show.S01E{}".to_string(),
            only_latest: true,
            ..Default::default()
        };
        let summary = summarize_rules(Some(&rules));
        assert!(summary.contains("目录 /anime"));
        assert!(summary.contains("模板 Show.S01E{}"));
        assert!(summary.contains("仅最新"));
    }
}
