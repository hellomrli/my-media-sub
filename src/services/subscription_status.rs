use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;

use crate::jobs::{Job, JobKind, JobStatus};
use crate::models::{
    AutomationEvent, AutomationStage, AutomationStatus, Notification, Settings, Subscription,
};
use crate::services::episode::{
    episode_state_key_with_context, episode_state_key_with_override, file_reference,
    known_episode_keys, progress_file_reference, transferred_episode_keys,
};

const MAX_EPISODE_GRID_ITEMS: usize = 500;
const MAX_ACTIVITY_ITEMS: usize = 30;

#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionDetail {
    pub subscription: Subscription,
    pub summary: SubscriptionStatusSummary,
    pub episodes: Vec<EpisodeStatusItem>,
    pub missing_episodes: Vec<i32>,
    pub pending_transfer_episodes: Vec<i32>,
    pub pending_download_episodes: Vec<i32>,
    pub pipeline: Vec<PipelineStep>,
    pub recent_jobs: Vec<Job>,
    pub recent_notifications: Vec<Notification>,
    pub recent_events: Vec<AutomationEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionStatusSummary {
    pub range_start: i32,
    pub range_end: i32,
    pub target_episode: Option<i32>,
    pub expected_count: usize,
    pub discovered_count: usize,
    pub transferred_count: usize,
    pub latest_discovered_episode: Option<i32>,
    pub latest_transferred_episode: Option<i32>,
    pub downloaded_count: usize,
    pub missing_count: usize,
    pub pending_transfer_count: usize,
    pub pending_download_count: usize,
    pub completion_percent: f64,
    pub data_inferred: bool,
    pub grid_truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EpisodeStatusItem {
    pub season: i32,
    pub episode: i32,
    pub discovered: bool,
    pub transferred: bool,
    pub download_status: String,
    pub missing: bool,
    pub recent: bool,
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineStep {
    pub id: String,
    pub label: String,
    pub status: String,
    pub message: String,
    pub count: usize,
}

#[derive(Default)]
struct EpisodeFiles {
    names: BTreeSet<String>,
    updated_at: Option<String>,
}

pub fn build_subscription_detail(
    subscription: Subscription,
    settings: &Settings,
    jobs: &[Job],
    notifications: &[Notification],
    events: &[AutomationEvent],
) -> SubscriptionDetail {
    let mut details = subscription.season_numbers().into_iter().map(|season| {
        build_season_detail(&subscription, season, settings, jobs, notifications, events)
    });
    let mut detail = details
        .next()
        .expect("subscriptions always include a season");
    for mut season in details {
        let summary = &mut detail.summary;
        let other = season.summary;
        summary.expected_count += other.expected_count;
        summary.discovered_count += other.discovered_count;
        summary.transferred_count += other.transferred_count;
        summary.downloaded_count += other.downloaded_count;
        summary.missing_count += other.missing_count;
        summary.pending_transfer_count += other.pending_transfer_count;
        summary.pending_download_count += other.pending_download_count;
        summary.range_end = summary.range_end.max(other.range_end);
        summary.data_inferred |= other.data_inferred;
        summary.grid_truncated |= other.grid_truncated;
        detail.episodes.append(&mut season.episodes);
        detail.missing_episodes.append(&mut season.missing_episodes);
        detail
            .pending_transfer_episodes
            .append(&mut season.pending_transfer_episodes);
        detail
            .pending_download_episodes
            .append(&mut season.pending_download_episodes);
    }
    if subscription.is_multi_season() {
        detail.summary.target_episode = None;
        let count = if subscription.sync_download_enabled {
            detail.summary.downloaded_count
        } else {
            detail.summary.transferred_count
        };
        detail.summary.completion_percent = if detail.summary.expected_count == 0 {
            0.0
        } else {
            count as f64 / detail.summary.expected_count as f64 * 100.0
        };
    }
    let downloads = detail
        .episodes
        .iter()
        .filter(|item| item.download_status == "queued")
        .enumerate()
        .map(|(index, _)| (index as i32, "queued"))
        .collect();
    detail.pipeline = build_pipeline(
        &subscription,
        &detail.summary,
        &detail.recent_jobs,
        &detail.recent_notifications,
        &downloads,
        &detail.recent_events,
    );
    detail.summary.grid_truncated |= detail.episodes.len() > MAX_EPISODE_GRID_ITEMS;
    detail.episodes.truncate(MAX_EPISODE_GRID_ITEMS);
    detail.subscription = subscription;
    detail
}

/// Project a subscription onto one season without assigning another season's
/// legacy counters or filenames to it. Shared by the detail view and calendar.
pub fn subscription_for_season(original: &Subscription, season: i32) -> Subscription {
    let mut sub = original.clone();
    sub.season = season;
    sub.season_end = None;
    sub.season_list = None;
    let belongs = |name: &str, parent: &str| {
        episode_state_key_with_context(name, parent, original.season, &original.rules.episode_regex)
            .is_some_and(|key| key.0 == season)
    };
    if original.media_type != "movie" {
        for names in [&mut sub.known_files, &mut sub.transferred_files] {
            names.retain(|name| belongs(name, ""));
            for name in names {
                *name = progress_file_reference(name, "", original.season);
            }
        }
    }
    sub.known_episodes = known_episode_keys(original)
        .into_iter()
        .filter_map(|(s, episode)| (s == season).then_some(episode))
        .collect();
    sub.transferred_file_keys = transferred_episode_keys(original)
        .into_iter()
        .filter(|(s, _)| *s == season)
        .map(|(_, episode)| format!("ep:{episode}"))
        .collect();
    if season != original.season_start() {
        sub.current_episode_number = sub.known_episodes.iter().copied().max().unwrap_or(0);
        sub.start_episode_number = None;
    }
    if !original.last_new_files.is_empty() || season != original.season_start() {
        sub.last_new_episodes = original
            .last_new_files
            .iter()
            .filter_map(|name| {
                episode_state_key_with_override(
                    name,
                    original.season,
                    &original.rules.episode_regex,
                )
                .filter(|key| key.0 == season)
                .map(|key| key.1)
            })
            .collect();
    }
    if let Some(probe) = &mut sub.last_probe {
        probe
            .files
            .retain(|file| belongs(&file.name, &file.parent_path));
        for file in &mut probe.files {
            file.name = file_reference(&file.name, &file.parent_path);
            file.parent_path.clear();
        }
    }
    sub.sync_downloads.retain(|record| {
        historical_notification_episode(original, season, &record.file_name, &record.target_dir)
            .is_some()
    });
    for record in &mut sub.sync_downloads {
        record.file_name = progress_file_reference(&record.file_name, &record.target_dir, season);
    }
    if original.is_multi_season() {
        sub.total_episode_number = original.rules.finish_after_episode.or_else(|| {
            crate::models::metadata::episode_count_for_season(original.metadata.as_ref(), season)
        });
    }
    sub
}

pub(crate) fn build_season_detail(
    original: &Subscription,
    season: i32,
    _settings: &Settings,
    jobs: &[Job],
    notifications: &[Notification],
    events: &[AutomationEvent],
) -> SubscriptionDetail {
    let subscription = subscription_for_season(original, season);
    let legacy_season = original.season_start();
    let episode_number = |name: &str| {
        episode_state_key_with_override(name, legacy_season, &subscription.rules.episode_regex)
            .filter(|key| key.0 == subscription.season)
            .map(|key| key.1)
    };
    let record_file =
        |files: &mut BTreeMap<i32, EpisodeFiles>, name: &str, updated: Option<&str>| {
            if let Some(episode) = episode_number(name) {
                add_episode_file(files, episode, name, updated);
            }
        };
    let subscription_id = subscription.id.as_str();
    let recent_jobs = jobs
        .iter()
        .filter(|job| job_matches_subscription(job, subscription_id))
        .take(MAX_ACTIVITY_ITEMS)
        .cloned()
        .collect::<Vec<_>>();
    let recent_notifications = notifications
        .iter()
        .filter(|notification| notification_matches_subscription(notification, subscription_id))
        .take(MAX_ACTIVITY_ITEMS)
        .cloned()
        .collect::<Vec<_>>();
    let recent_events = events
        .iter()
        .filter(|event| event.subscription_id.as_deref() == Some(subscription_id))
        .take(MAX_ACTIVITY_ITEMS * 4)
        .cloned()
        .collect::<Vec<_>>();

    let mut files_by_episode = BTreeMap::<i32, EpisodeFiles>::new();
    let mut discovered = subscription
        .known_episodes
        .iter()
        .copied()
        .filter(|episode| *episode > 0)
        .collect::<BTreeSet<_>>();
    let mut data_inferred = false;

    if discovered.is_empty() && subscription.current_episode_number > 0 {
        let start = subscription.start_episode_number.unwrap_or(1).max(1);
        discovered.extend(start..=subscription.current_episode_number);
        data_inferred = true;
    }

    for name in &subscription.known_files {
        record_file(&mut files_by_episode, name, None);
        if let Some(episode) = episode_number(name) {
            discovered.insert(episode);
        }
    }
    if let Some(probe) = &subscription.last_probe {
        for file in &probe.files {
            if file.is_dir {
                continue;
            }
            record_file(
                &mut files_by_episode,
                &file.name,
                file.updated_at.as_deref(),
            );
            if let Some(episode) = episode_number(&file.name) {
                discovered.insert(episode);
            }
        }
    }

    let mut transferred = subscription
        .transferred_file_keys
        .iter()
        .filter_map(|key| key.strip_prefix("ep:")?.parse::<i32>().ok())
        .filter(|episode| *episode > 0)
        .collect::<BTreeSet<_>>();
    for name in &subscription.transferred_files {
        record_file(&mut files_by_episode, name, None);
        if let Some(episode) = episode_number(name) {
            transferred.insert(episode);
            discovered.insert(episode);
        }
    }

    let mut completed_gids = notifications
        .iter()
        .filter(|notification| notification.event == "download_completed")
        .filter_map(|notification| meta_string(&notification.meta, "gid"))
        .collect::<HashSet<_>>();
    let mut completed_file_names = notifications
        .iter()
        .filter(|notification| notification.event == "download_completed")
        .filter_map(|notification| meta_string(&notification.meta, "file_name"))
        .collect::<HashSet<_>>();
    for notification in notifications
        .iter()
        .filter(|notification| notification.event == "download_completed")
    {
        if let Some(gids) = notification.meta.get("gids").and_then(Value::as_array) {
            for gid in gids.iter().filter_map(Value::as_str) {
                completed_gids.insert(gid.to_string());
            }
        }
        if let Some(files) = notification.meta.get("files").and_then(Value::as_array) {
            for file in files
                .iter()
                .filter_map(|file| file.get("file_name").and_then(Value::as_str))
            {
                completed_file_names.insert(file.to_string());
            }
        }
    }
    let mut download_status = HashMap::<i32, &'static str>::new();

    // 新版本把 Aria2 关联作为订阅业务状态持久化；通知仅作为旧数据兼容
    // 和展示审计来源，清空通知不会再让下载进度消失。
    for record in &subscription.sync_downloads {
        record_file(&mut files_by_episode, &record.file_name, None);
        let Some(episode) = episode_number(&record.file_name) else {
            continue;
        };
        let status = if record.completed_at.is_some() {
            "completed"
        } else {
            "queued"
        };
        set_episode_status(&mut download_status, episode, status);
    }

    for event in &recent_events {
        // Unscoped historical events remain in the timeline, but cannot prove
        // which season was transferred or downloaded.
        if event.metadata.get("season").and_then(Value::as_i64)
            != Some(i64::from(subscription.season))
        {
            continue;
        }
        let Some(episode) = event.episode.filter(|episode| *episode > 0) else {
            continue;
        };
        match (event.stage, event.status) {
            (AutomationStage::CloudTransfer, AutomationStatus::Succeeded) => {
                transferred.insert(episode);
                discovered.insert(episode);
            }
            (AutomationStage::Aria2, AutomationStatus::Succeeded) => {
                download_status.entry(episode).or_insert("queued");
            }
            (AutomationStage::Aria2, AutomationStatus::Failed) => {
                download_status.entry(episode).or_insert("failed");
            }
            _ => {}
        }
    }

    for notification in &recent_notifications {
        if notification.event != "subscription_transferred" {
            continue;
        }
        let file_names = meta_string_array(&notification.meta, "file_names");
        for name in &file_names {
            if let Some(episode) = historical_notification_episode(original, season, name, "") {
                add_episode_file(&mut files_by_episode, episode, name, None);
                discovered.insert(episode);
                transferred.insert(episode);
            }
        }

        if let Some(downloads) = notification
            .meta
            .get("sync_downloads")
            .and_then(Value::as_array)
        {
            for item in downloads {
                let Some(file_name) = item.get("file_name").and_then(Value::as_str) else {
                    continue;
                };
                let gid = item.get("gid").and_then(Value::as_str).unwrap_or_default();
                let record = original
                    .sync_downloads
                    .iter()
                    .find(|record| !gid.is_empty() && record.gid == gid);
                let (name, parent) = if let Some(record) = record {
                    (record.file_name.as_str(), record.target_dir.as_str())
                } else {
                    (
                        file_name,
                        item.get("download_dir")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
                };
                let Some(episode) = historical_notification_episode(original, season, name, parent)
                else {
                    continue;
                };
                let status = if completed_gids.contains(gid)
                    || (gid.is_empty() && completed_file_names.contains(file_name))
                {
                    "completed"
                } else {
                    "queued"
                };
                set_episode_status(&mut download_status, episode, status);
            }
        }
    }

    let start = subscription.start_episode_number.unwrap_or(1).max(1);
    let target_episode = subscription
        .total_episode_number
        .or(subscription.rules.finish_after_episode)
        .filter(|episode| *episode >= start);
    let observed_end = discovered
        .iter()
        .chain(transferred.iter())
        .copied()
        .chain(std::iter::once(subscription.current_episode_number))
        .max()
        .unwrap_or(0)
        .max(start - 1);
    let range_end = target_episode.unwrap_or(observed_end).max(observed_end);

    let expected = target_episode
        .map(|target| (start..=target).collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let missing_episodes = expected
        .difference(&discovered)
        .copied()
        .collect::<Vec<_>>();
    let pending_transfer_episodes = discovered
        .difference(&transferred)
        .filter(|episode| {
            target_episode
                .map(|target| **episode <= target)
                .unwrap_or(true)
        })
        .copied()
        .collect::<Vec<_>>();
    let downloaded = download_status
        .iter()
        .filter_map(|(episode, status)| (*status == "completed").then_some(*episode))
        .collect::<BTreeSet<_>>();
    let pending_download_episodes = if subscription.sync_download_enabled {
        transferred
            .difference(&downloaded)
            .filter(|episode| {
                target_episode
                    .map(|target| **episode <= target)
                    .unwrap_or(true)
            })
            .copied()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let mut episode_numbers = if let Some(target) = target_episode {
        (start..=target).collect::<BTreeSet<_>>()
    } else {
        discovered
            .union(&transferred)
            .copied()
            .collect::<BTreeSet<_>>()
    };
    episode_numbers.extend(discovered.iter().copied());
    episode_numbers.extend(transferred.iter().copied());
    let grid_truncated = episode_numbers.len() > MAX_EPISODE_GRID_ITEMS;
    let recent_episodes = subscription
        .last_new_episodes
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let episodes = episode_numbers
        .into_iter()
        .take(MAX_EPISODE_GRID_ITEMS)
        .map(|episode| {
            let transferred_episode = transferred.contains(&episode);
            let download = if !subscription.sync_download_enabled {
                "disabled"
            } else {
                download_status
                    .get(&episode)
                    .copied()
                    .unwrap_or(if transferred_episode {
                        "pending"
                    } else {
                        "not_started"
                    })
            };
            let files = files_by_episode.remove(&episode).unwrap_or_default();
            EpisodeStatusItem {
                season: subscription.season,
                episode,
                discovered: discovered.contains(&episode),
                transferred: transferred_episode,
                download_status: download.to_string(),
                missing: target_episode.is_some() && !discovered.contains(&episode),
                recent: recent_episodes.contains(&episode),
                files: files.names.into_iter().collect(),
                updated_at: files.updated_at,
            }
        })
        .collect::<Vec<_>>();

    let expected_count = expected.len();
    let discovered_count = if target_episode.is_some() {
        expected.intersection(&discovered).count()
    } else {
        discovered.len()
    };
    let transferred_count = if target_episode.is_some() {
        expected.intersection(&transferred).count()
    } else {
        transferred.len()
    };
    let downloaded_count = if target_episode.is_some() {
        expected.intersection(&downloaded).count()
    } else {
        downloaded.len()
    };
    let progress_base = if subscription.sync_download_enabled {
        downloaded_count
    } else {
        transferred_count
    };
    let completion_percent = if expected_count > 0 {
        progress_base as f64 / expected_count as f64 * 100.0
    } else {
        0.0
    };

    let summary = SubscriptionStatusSummary {
        range_start: start,
        range_end,
        target_episode,
        expected_count,
        discovered_count,
        transferred_count,
        latest_discovered_episode: discovered.last().copied(),
        latest_transferred_episode: transferred.last().copied(),
        downloaded_count,
        missing_count: missing_episodes.len(),
        pending_transfer_count: pending_transfer_episodes.len(),
        pending_download_count: pending_download_episodes.len(),
        completion_percent,
        data_inferred,
        grid_truncated,
    };
    let pipeline = build_pipeline(
        &subscription,
        &summary,
        &recent_jobs,
        &recent_notifications,
        &download_status,
        &recent_events,
    );

    SubscriptionDetail {
        subscription,
        summary,
        episodes,
        missing_episodes,
        pending_transfer_episodes,
        pending_download_episodes,
        pipeline,
        recent_jobs,
        recent_notifications,
        recent_events,
    }
}

// Old notifications may only have basenames. Prefer their persisted history
// when it identifies a season; ambiguous names cannot prove cross-season state.
fn historical_notification_episode(
    subscription: &Subscription,
    season: i32,
    name: &str,
    parent: &str,
) -> Option<i32> {
    crate::services::episode::historical_episode_key(subscription, name, parent)
        .filter(|key| key.0 == season)
        .map(|key| key.1)
}

fn add_episode_file(
    files_by_episode: &mut BTreeMap<i32, EpisodeFiles>,
    episode: i32,
    name: &str,
    updated_at: Option<&str>,
) {
    let entry = files_by_episode.entry(episode).or_default();
    if !name.trim().is_empty() {
        entry.names.insert(name.to_string());
    }
    if let Some(updated_at) = updated_at.filter(|value| !value.trim().is_empty()) {
        if entry.updated_at.as_deref().unwrap_or_default() < updated_at {
            entry.updated_at = Some(updated_at.to_string());
        }
    }
}

fn set_episode_status(
    statuses: &mut HashMap<i32, &'static str>,
    episode: i32,
    status: &'static str,
) {
    let rank = |value: &str| match value {
        "failed" => 4,
        "completed" => 3,
        "queued" => 2,
        "pending" => 1,
        _ => 0,
    };
    if statuses
        .get(&episode)
        .map(|current| rank(current) >= rank(status))
        .unwrap_or(false)
    {
        return;
    }
    statuses.insert(episode, status);
}

fn job_matches_subscription(job: &Job, subscription_id: &str) -> bool {
    json_subscription_id(&job.payload) == Some(subscription_id)
        || job.result.as_ref().and_then(json_subscription_id) == Some(subscription_id)
}

fn notification_matches_subscription(notification: &Notification, subscription_id: &str) -> bool {
    notification
        .meta
        .get("subscription_id")
        .and_then(Value::as_str)
        == Some(subscription_id)
}

fn json_subscription_id(value: &Value) -> Option<&str> {
    value.get("subscription_id").and_then(Value::as_str)
}

fn meta_string(meta: &HashMap<String, Value>, key: &str) -> Option<String> {
    meta.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

fn meta_string_array(meta: &HashMap<String, Value>, key: &str) -> Vec<String> {
    meta.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn build_pipeline(
    subscription: &Subscription,
    summary: &SubscriptionStatusSummary,
    jobs: &[Job],
    notifications: &[Notification],
    download_status: &HashMap<i32, &'static str>,
    events: &[AutomationEvent],
) -> Vec<PipelineStep> {
    let latest_transfer_job = jobs
        .iter()
        .find(|job| job.kind == JobKind::SubscriptionTransfer);
    let transfer_status = match latest_transfer_job.map(|job| &job.status) {
        Some(JobStatus::Queued | JobStatus::Running) => "active",
        Some(JobStatus::Failed) => "error",
        _ if summary.transferred_count > 0 => "success",
        _ if subscription.notify_only => "disabled",
        _ => "idle",
    };
    let transfer_message = match latest_transfer_job {
        Some(job) if matches!(job.status, JobStatus::Queued | JobStatus::Running) => {
            job.message.clone()
        }
        Some(job) if job.status == JobStatus::Failed => {
            job.error.clone().unwrap_or_else(|| job.message.clone())
        }
        _ if subscription.notify_only => "仅通知模式，不执行自动转存".to_string(),
        _ if summary.transferred_count > 0 => {
            format!("已转存 {} 集", summary.transferred_count)
        }
        _ => "等待发现可转存内容".to_string(),
    };

    let queued_downloads = download_status
        .values()
        .filter(|status| **status == "queued")
        .count();
    let download_pipeline_status = if !subscription.sync_download_enabled {
        "disabled"
    } else if summary.pending_download_count == 0 && summary.downloaded_count > 0 {
        "success"
    } else if queued_downloads > 0 {
        "active"
    } else if summary.pending_download_count > 0 {
        "warning"
    } else {
        "idle"
    };

    let mut pipeline = vec![
        PipelineStep {
            id: "discover".to_string(),
            label: "发现更新".to_string(),
            status: if subscription.status == "invalid" {
                "error"
            } else if summary.discovered_count > 0 {
                "success"
            } else {
                "idle"
            }
            .to_string(),
            message: if subscription.status == "invalid" {
                subscription.last_error.clone()
            } else if summary.missing_count > 0 {
                format!(
                    "已发现 {} 集，仍缺 {} 集",
                    summary.discovered_count, summary.missing_count
                )
            } else {
                format!("已发现 {} 集", summary.discovered_count)
            },
            count: summary.discovered_count,
        },
        PipelineStep {
            id: "filter".to_string(),
            label: "文件过滤".to_string(),
            status: if subscription.last_checked_at > 0 {
                "success"
            } else {
                "idle"
            }
            .to_string(),
            message: if subscription.last_plan_summary.trim().is_empty() {
                subscription.last_check_summary.clone()
            } else {
                subscription.last_plan_summary.clone()
            },
            count: summary.discovered_count,
        },
        PipelineStep {
            id: "transfer".to_string(),
            label: "夸克转存".to_string(),
            status: transfer_status.to_string(),
            message: transfer_message,
            count: summary.transferred_count,
        },
        PipelineStep {
            id: "rename".to_string(),
            label: "重命名".to_string(),
            status: if summary.transferred_count > 0 {
                "success"
            } else {
                "idle"
            }
            .to_string(),
            message: if summary.transferred_count > 0 {
                format!("{} 集已进入命名规则", summary.transferred_count)
            } else {
                "等待转存完成".to_string()
            },
            count: summary.transferred_count,
        },
        PipelineStep {
            id: "aria2".to_string(),
            label: "Aria2".to_string(),
            status: download_pipeline_status.to_string(),
            message: if !subscription.sync_download_enabled {
                "未启用同步下载".to_string()
            } else if queued_downloads > 0 {
                format!("{} 集已提交，等待下载完成", queued_downloads)
            } else if summary.downloaded_count > 0 {
                format!("已确认下载 {} 集", summary.downloaded_count)
            } else {
                "等待转存后提交下载".to_string()
            },
            count: summary.downloaded_count,
        },
        PipelineStep {
            id: "notify".to_string(),
            label: "通知".to_string(),
            status: if notifications.is_empty() {
                "idle"
            } else {
                "success"
            }
            .to_string(),
            message: if notifications.is_empty() {
                "暂无订阅通知记录".to_string()
            } else {
                format!("最近保留 {} 条相关通知", notifications.len())
            },
            count: notifications.len(),
        },
    ];

    for event in events.iter().rev() {
        let step_id = match event.stage {
            AutomationStage::SourceCheck => "discover",
            AutomationStage::FileFilter | AutomationStage::VersionSelect => "filter",
            AutomationStage::CloudTransfer => "transfer",
            AutomationStage::Rename => "rename",
            AutomationStage::Aria2 => "aria2",
            AutomationStage::Notification => "notify",
        };
        let Some(step) = pipeline.iter_mut().find(|step| step.id == step_id) else {
            continue;
        };
        step.status = match event.status {
            AutomationStatus::Succeeded => "success",
            AutomationStatus::Failed | AutomationStatus::Canceled => "error",
            AutomationStatus::Running | AutomationStatus::Retrying => "active",
            AutomationStatus::Skipped => "disabled",
            AutomationStatus::Pending => "idle",
        }
        .to_string();
        step.message = if event.error.trim().is_empty() {
            event.message.clone()
        } else {
            event.error.clone()
        };
        if let Some(count) = event.metadata.get("count").and_then(Value::as_u64) {
            step.count = count as usize;
        }
    }

    pipeline
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TransferRules;
    use serde_json::json;

    fn subscription() -> Subscription {
        serde_json::from_value(json!({
            "id": "sub-1",
            "title": "Example",
            "media_type": "series",
            "season": 1,
            "start_episode_number": 1,
            "current_episode_number": 4,
            "total_episode_number": 6,
            "url": "https://pan.quark.cn/s/example",
            "known_files": ["Example.S01E01.mkv", "Example.S01E02.mkv", "Example.S01E04.mkv"],
            "known_episodes": [1, 2, 4],
            "transferred_files": ["Example.S01E01.mkv", "Example.S01E02.mkv"],
            "transferred_file_keys": ["ep:1", "ep:2"],
            "sync_download_enabled": true,
            "enabled": true,
            "completed": false,
            "rules": TransferRules::default(),
            "created_at": 1,
            "updated_at": 2,
            "last_checked_at": 3,
            "last_new_episodes": [4],
            "status": "active"
        }))
        .unwrap()
    }

    fn notification(event: &str, meta: Value) -> Notification {
        Notification {
            id: uuid::Uuid::new_v4().to_string(),
            level: "success".to_string(),
            event: event.to_string(),
            title: event.to_string(),
            message: "处理完成".to_string(),
            meta: serde_json::from_value(meta).unwrap(),
            read: false,
            created_at: 10,
        }
    }

    #[test]
    fn builds_missing_transfer_and_download_states() {
        let transfer = notification(
            "subscription_transferred",
            json!({
                "subscription_id": "sub-1",
                "file_names": ["Example.S01E01.mkv", "Example.S01E02.mkv"],
                "sync_downloads": [
                    {"gid": "gid-1", "file_name": "Example.S01E01.mkv"},
                    {"gid": "gid-2", "file_name": "Example.S01E02.mkv"}
                ],
            }),
        );
        let completed = notification(
            "download_completed",
            json!({
                "gid": "gid-1",
                "file_name": "Example.S01E01.mkv"
            }),
        );
        let settings = Settings::default();
        let detail =
            build_subscription_detail(subscription(), &settings, &[], &[transfer, completed], &[]);

        assert_eq!(detail.missing_episodes, vec![3, 5, 6]);
        assert_eq!(detail.pending_transfer_episodes, vec![4]);
        assert_eq!(detail.summary.downloaded_count, 1);
        assert_eq!(detail.episodes[0].download_status, "completed");
        assert_eq!(detail.episodes[1].download_status, "queued");
        assert!(detail.episodes[3].recent);
    }

    #[test]
    fn completed_download_with_same_basename_does_not_complete_another_season() {
        let mut sub = subscription();
        sub.season_end = Some(3);
        sub.season_list = Some(vec![1, 3]);
        sub.transferred_files = vec!["Season 1/01.mkv".into(), "Season 3/01.mkv".into()];
        sub.sync_downloads = [1, 3]
            .into_iter()
            .map(|season| crate::models::SyncDownloadRecord {
                gid: format!("gid-{season}"),
                file_name: "01.mkv".into(),
                download_dir: format!("/downloads/Season {season}"),
                target_dir: format!("/series/Example/Season {season}"),
                submitted_at: 1,
                completed_at: (season == 3).then_some(2),
            })
            .collect();
        let transfer = notification(
            "subscription_transferred",
            json!({
                "subscription_id": "sub-1",
                "file_names": ["Season 1/01.mkv", "Season 3/01.mkv"],
                "sync_downloads": [
                    {"gid":"gid-1", "file_name":"01.mkv"},
                    {"gid":"gid-3", "file_name":"01.mkv"}
                ]
            }),
        );
        let completed = notification(
            "download_completed",
            json!({
                "gid": "gid-3", "file_name": "01.mkv"
            }),
        );
        let detail =
            build_subscription_detail(sub, &Settings::default(), &[], &[transfer, completed], &[]);
        let first = detail
            .episodes
            .iter()
            .find(|item| item.season == 1 && item.episode == 1)
            .unwrap();
        let third = detail
            .episodes
            .iter()
            .find(|item| item.season == 3 && item.episode == 1)
            .unwrap();
        assert_eq!(first.download_status, "queued");
        assert_eq!(third.download_status, "completed");
    }

    #[test]
    fn persisted_download_records_survive_notification_cleanup() {
        let mut sub = subscription();
        sub.sync_downloads = vec![
            crate::models::SyncDownloadRecord {
                gid: "gid-1".to_string(),
                file_name: "Example.S01E01.mkv".to_string(),
                download_dir: "/downloads".to_string(),
                target_dir: "/series/Example/Season 1".to_string(),
                submitted_at: 1,
                completed_at: Some(2),
            },
            crate::models::SyncDownloadRecord {
                gid: "gid-2".to_string(),
                file_name: "Example.S01E02.mkv".to_string(),
                download_dir: "/downloads".to_string(),
                target_dir: "/series/Example/Season 1".to_string(),
                submitted_at: 1,
                completed_at: None,
            },
        ];

        let detail = build_subscription_detail(sub, &Settings::default(), &[], &[], &[]);

        assert_eq!(detail.summary.downloaded_count, 1);
        assert_eq!(detail.episodes[0].download_status, "completed");
        assert_eq!(detail.episodes[1].download_status, "queued");
    }

    #[test]
    fn infers_contiguous_progress_for_legacy_subscriptions_without_known_episodes() {
        let mut sub = subscription();
        sub.known_files.clear();
        sub.known_episodes.clear();
        sub.current_episode_number = 3;
        sub.total_episode_number = Some(4);
        let detail = build_subscription_detail(sub, &Settings::default(), &[], &[], &[]);
        assert!(detail.summary.data_inferred);
        assert_eq!(detail.summary.discovered_count, 3);
        assert_eq!(detail.missing_episodes, vec![4]);
    }
}
