use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;

use crate::jobs::{Job, JobKind, JobStatus};
use crate::models::{
    AutomationEvent, AutomationStage, AutomationStatus, Notification, Settings, Subscription,
};
use crate::services::detect_episode;

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
    pub downloaded_count: usize,
    pub strm_count: usize,
    pub missing_count: usize,
    pub pending_transfer_count: usize,
    pub pending_download_count: usize,
    pub completion_percent: f64,
    pub data_inferred: bool,
    pub grid_truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EpisodeStatusItem {
    pub episode: i32,
    pub discovered: bool,
    pub transferred: bool,
    pub download_status: String,
    pub strm_status: String,
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
        add_episode_file(&mut files_by_episode, name, None);
        if let Some(episode) = episode_number(name) {
            discovered.insert(episode);
        }
    }
    if let Some(probe) = &subscription.last_probe {
        for file in &probe.files {
            if file.is_dir {
                continue;
            }
            add_episode_file(
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
        add_episode_file(&mut files_by_episode, name, None);
        if let Some(episode) = episode_number(name) {
            transferred.insert(episode);
            discovered.insert(episode);
        }
    }

    let completed_gids = notifications
        .iter()
        .filter(|notification| notification.event == "download_completed")
        .filter_map(|notification| meta_string(&notification.meta, "gid"))
        .collect::<HashSet<_>>();
    let completed_file_names = notifications
        .iter()
        .filter(|notification| notification.event == "download_completed")
        .filter_map(|notification| meta_string(&notification.meta, "file_name"))
        .collect::<HashSet<_>>();
    let mut download_status = HashMap::<i32, &'static str>::new();
    let mut strm_status = HashMap::<i32, &'static str>::new();

    for event in &recent_events {
        let Some(episode) = event.episode.filter(|episode| *episode > 0) else {
            continue;
        };
        match (event.stage, event.status) {
            (AutomationStage::CloudTransfer, AutomationStatus::Succeeded) => {
                transferred.insert(episode);
                discovered.insert(episode);
            }
            (AutomationStage::Strm, AutomationStatus::Succeeded) => {
                strm_status.entry(episode).or_insert("generated");
            }
            (AutomationStage::Strm, AutomationStatus::Failed) => {
                strm_status.entry(episode).or_insert("failed");
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
            add_episode_file(&mut files_by_episode, name, None);
            if let Some(episode) = episode_number(name) {
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
                let Some(episode) = episode_number(file_name) else {
                    continue;
                };
                let gid = item.get("gid").and_then(Value::as_str).unwrap_or_default();
                let status =
                    if completed_gids.contains(gid) || completed_file_names.contains(file_name) {
                        "completed"
                    } else {
                        "queued"
                    };
                set_episode_status(&mut download_status, episode, status);
            }
        }

        let strm_error = notification
            .meta
            .get("strm_error")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let strm_generated = notification
            .meta
            .get("strm_generated_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0;
        let inferred_strm_status = if strm_error.is_some() {
            Some("failed")
        } else if strm_generated {
            Some("generated")
        } else {
            None
        };
        if let Some(status) = inferred_strm_status {
            for name in &file_names {
                if let Some(episode) = episode_number(name) {
                    // Notifications are newest-first; the newest STRM outcome wins.
                    strm_status.entry(episode).or_insert(status);
                }
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
    let generated_strm = strm_status
        .iter()
        .filter_map(|(episode, status)| (*status == "generated").then_some(*episode))
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
            let strm = if !settings.strm_enabled || !subscription.strm_enabled {
                "disabled"
            } else {
                strm_status
                    .get(&episode)
                    .copied()
                    .unwrap_or(if transferred_episode {
                        "unknown"
                    } else {
                        "not_started"
                    })
            };
            let files = files_by_episode.remove(&episode).unwrap_or_default();
            EpisodeStatusItem {
                episode,
                discovered: discovered.contains(&episode),
                transferred: transferred_episode,
                download_status: download.to_string(),
                strm_status: strm.to_string(),
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
    let strm_count = if target_episode.is_some() {
        expected.intersection(&generated_strm).count()
    } else {
        generated_strm.len()
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
        downloaded_count,
        strm_count,
        missing_count: missing_episodes.len(),
        pending_transfer_count: pending_transfer_episodes.len(),
        pending_download_count: pending_download_episodes.len(),
        completion_percent,
        data_inferred,
        grid_truncated,
    };
    let pipeline = build_pipeline(
        &subscription,
        settings,
        &summary,
        &recent_jobs,
        &recent_notifications,
        &download_status,
        &strm_status,
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

fn add_episode_file(
    files_by_episode: &mut BTreeMap<i32, EpisodeFiles>,
    name: &str,
    updated_at: Option<&str>,
) {
    let Some(episode) = episode_number(name) else {
        return;
    };
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

fn episode_number(name: &str) -> Option<i32> {
    detect_episode(name).episode.filter(|episode| *episode > 0)
}

fn set_episode_status(
    statuses: &mut HashMap<i32, &'static str>,
    episode: i32,
    status: &'static str,
) {
    let rank = |value: &str| match value {
        "failed" => 4,
        "completed" | "generated" => 3,
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
    settings: &Settings,
    summary: &SubscriptionStatusSummary,
    jobs: &[Job],
    notifications: &[Notification],
    download_status: &HashMap<i32, &'static str>,
    strm_status: &HashMap<i32, &'static str>,
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

    let strm_failed = strm_status.values().any(|status| *status == "failed");
    let strm_pipeline_status = if !settings.strm_enabled || !subscription.strm_enabled {
        "disabled"
    } else if strm_failed {
        "error"
    } else if summary.strm_count > 0 {
        "success"
    } else if summary.transferred_count > 0 {
        "warning"
    } else {
        "idle"
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
            id: "strm".to_string(),
            label: "STRM".to_string(),
            status: strm_pipeline_status.to_string(),
            message: if !settings.strm_enabled || !subscription.strm_enabled {
                "未启用 STRM".to_string()
            } else if strm_failed {
                "最近一次 STRM 生成失败".to_string()
            } else if summary.strm_count > 0 {
                format!("已确认生成 {} 集", summary.strm_count)
            } else {
                "暂无可确认的生成记录".to_string()
            },
            count: summary.strm_count,
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
            AutomationStage::Strm => "strm",
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
            "strm_enabled": true,
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
            message: "已生成 2 个 STRM 文件".to_string(),
            meta: serde_json::from_value(meta).unwrap(),
            read: false,
            created_at: 10,
        }
    }

    #[test]
    fn builds_missing_transfer_download_and_strm_states() {
        let transfer = notification(
            "subscription_transferred",
            json!({
                "subscription_id": "sub-1",
                "file_names": ["Example.S01E01.mkv", "Example.S01E02.mkv"],
                "sync_downloads": [
                    {"gid": "gid-1", "file_name": "Example.S01E01.mkv"},
                    {"gid": "gid-2", "file_name": "Example.S01E02.mkv"}
                ],
                "strm_generated_count": 2
            }),
        );
        let completed = notification(
            "download_completed",
            json!({
                "gid": "gid-1",
                "file_name": "Example.S01E01.mkv"
            }),
        );
        let settings = Settings {
            strm_enabled: true,
            ..Settings::default()
        };
        let detail =
            build_subscription_detail(subscription(), &settings, &[], &[transfer, completed], &[]);

        assert_eq!(detail.missing_episodes, vec![3, 5, 6]);
        assert_eq!(detail.pending_transfer_episodes, vec![4]);
        assert_eq!(detail.summary.downloaded_count, 1);
        assert_eq!(detail.summary.strm_count, 2);
        assert_eq!(detail.episodes[0].download_status, "completed");
        assert_eq!(detail.episodes[1].download_status, "queued");
        assert!(detail.episodes[3].recent);
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
