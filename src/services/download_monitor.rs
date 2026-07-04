use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::RwLock;
use tracing::warn;

use crate::clients::aria2::Aria2Task;
use crate::clients::Aria2Client;
use crate::error::{AppError, Result};
use crate::jobs::JobQueue;
use crate::models::{Notification, Settings, Subscription};
use crate::services::notification::{
    add_notification, dispatch_push_event_for_notification, PushDispatchRequest,
};
use crate::services::push::{PushEvent, PushLevel};
use crate::services::subscription_progress::{
    completion_target_episode, should_mark_completed_from_file_names,
};
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};
use crate::utils::unix_now;

const MONITOR_INTERVAL: Duration = Duration::from_secs(15);
const STOPPED_LIMIT: u64 = 50;

/// 后台监控 Aria2 已停止任务，并在下载完成时发出通知。
pub struct DownloadMonitorService {
    settings_store: Arc<SettingsStore>,
    subscription_store: Arc<SubscriptionStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Arc<JobQueue>,
    notified_completed_downloads: RwLock<HashSet<String>>,
}

impl DownloadMonitorService {
    pub fn new(
        settings_store: Arc<SettingsStore>,
        subscription_store: Arc<SubscriptionStore>,
        notification_store: Arc<NotificationStore>,
        job_queue: Arc<JobQueue>,
    ) -> Self {
        Self {
            settings_store,
            subscription_store,
            notification_store,
            job_queue,
            notified_completed_downloads: RwLock::new(HashSet::new()),
        }
    }

    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(MONITOR_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                ticker.tick().await;
                if let Err(error) = self.poll_once(STOPPED_LIMIT).await {
                    warn!("Aria2 下载完成监控失败: {}", error);
                }
            }
        });
    }

    pub async fn poll_once(&self, stopped_limit: u64) -> Result<()> {
        let settings = self.settings_store.get().await;
        if settings.aria2_rpc_url.trim().is_empty() {
            return Ok(());
        }

        let aria2 = aria2_client(&settings)?;
        let tasks = aria2.list_tasks(stopped_limit.clamp(1, 50)).await?;
        self.notify_completed_downloads(&tasks.stopped).await;
        Ok(())
    }

    pub async fn notify_completed_downloads(&self, tasks: &[Aria2Task]) {
        let history = self.notification_store.list(true).await;
        let pushed_downloads = self
            .job_queue
            .successful_push_dispatch_messages(PushEvent::DownloadCompleted.as_str())
            .await;
        let known_keys = self.notified_completed_downloads.read().await.clone();
        let pending_tasks = tasks
            .iter()
            .filter(|task| task.status == "complete")
            .filter(|task| !task.gid.trim().is_empty())
            .filter(|task| {
                download_completed_dedupe_keys(task)
                    .iter()
                    .all(|key| !known_keys.contains(key))
            })
            .filter(|task| !completed_download_already_recorded(&history, &pushed_downloads, task))
            .collect::<Vec<_>>();

        if pending_tasks.is_empty() {
            return;
        }

        let mut inserted_gids = Vec::new();
        {
            let mut known = self.notified_completed_downloads.write().await;
            for task in pending_tasks {
                let keys = download_completed_dedupe_keys(task);
                if keys.iter().any(|key| known.contains(key)) {
                    continue;
                }
                for key in keys {
                    known.insert(key);
                }
                inserted_gids.push(task.gid.clone());
            }
        }

        for gid in inserted_gids {
            if let Some(task) = tasks.iter().find(|task| task.gid == gid) {
                if let Err(e) = self.notify_completed_download(task).await {
                    warn!("记录 Aria2 下载完成通知失败 {}: {}", task.gid, e);
                }
            }
        }
    }

    async fn notify_completed_download(&self, task: &Aria2Task) -> Result<()> {
        let (title, message) = download_completed_title_message(task);
        let meta = download_completed_meta(task);
        let notification = add_notification(
            &self.notification_store,
            "success",
            PushEvent::DownloadCompleted.as_str(),
            title.clone(),
            message.clone(),
            meta,
        )
        .await?;
        dispatch_push_event_for_notification(
            self.settings_store.clone(),
            self.notification_store.clone(),
            Some(self.job_queue.clone()),
            PushDispatchRequest {
                notification_id: Some(notification.id),
                event: PushEvent::DownloadCompleted,
                title,
                message,
                level: PushLevel::Success,
            },
        )
        .await;

        if let Err(e) = self.complete_subscription_for_download(task).await {
            warn!("根据下载完成更新订阅状态失败 {}: {}", task.gid, e);
        }

        Ok(())
    }

    async fn complete_subscription_for_download(&self, task: &Aria2Task) -> Result<()> {
        let history = self.notification_store.list(true).await;
        let gid = task.gid.trim();
        if gid.is_empty() {
            return Ok(());
        }

        let completed_gids = download_completed_gids(&history, gid);
        let Some(subscription_id) = subscription_id_for_download_gid(&history, gid) else {
            return Ok(());
        };
        let Some(sub) = self.subscription_store.get(&subscription_id).await else {
            return Ok(());
        };
        if sub.completed || !sub.sync_download_enabled {
            return Ok(());
        }

        let completed_files =
            completed_subscription_download_files(&history, &subscription_id, &completed_gids);
        if !should_mark_completed_from_file_names(&sub, &completed_files) {
            return Ok(());
        }

        self.mark_subscription_completed_after_download(&sub, &completed_files)
            .await?;
        Ok(())
    }

    async fn mark_subscription_completed_after_download(
        &self,
        sub: &Subscription,
        completed_files: &[String],
    ) -> Result<bool> {
        let target_episode = completion_target_episode(sub);
        let now = now_ts();
        let updated = self
            .subscription_store
            .update(&sub.id, |sub| {
                if sub.completed {
                    return;
                }
                sub.completed = true;
                sub.status = "completed".to_string();
                sub.invalid_since = None;
                sub.last_error = String::new();
                if let Some(target_episode) = target_episode {
                    sub.current_episode_number = sub.current_episode_number.max(target_episode);
                }
                if sub.total_episode_number.is_none() {
                    sub.total_episode_number = sub.rules.finish_after_episode;
                }
                sub.updated_at = now;
            })
            .await?
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

        if sub.completed || !updated.completed {
            return Ok(false);
        }

        let total = completion_target_episode(&updated).unwrap_or(updated.current_episode_number);
        let title = format!("订阅已完结: {}", updated.title);
        let message = if total > 0 {
            format!("已下载到第 {} 集", total)
        } else {
            "订阅已标记为完结".to_string()
        };
        let meta: HashMap<String, Value> = HashMap::from([
            ("subscription_id".to_string(), json!(updated.id)),
            ("subscription_title".to_string(), json!(updated.title)),
            (
                "completed_download_files".to_string(),
                json!(completed_files),
            ),
        ]);

        let notification = add_notification(
            &self.notification_store,
            "success",
            PushEvent::SubscriptionCompleted.as_str(),
            title.clone(),
            message.clone(),
            meta,
        )
        .await?;
        dispatch_push_event_for_notification(
            self.settings_store.clone(),
            self.notification_store.clone(),
            Some(self.job_queue.clone()),
            PushDispatchRequest {
                notification_id: Some(notification.id),
                event: PushEvent::SubscriptionCompleted,
                title,
                message,
                level: PushLevel::Success,
            },
        )
        .await;

        Ok(true)
    }
}

fn aria2_client(settings: &Settings) -> Result<Aria2Client> {
    if settings.aria2_rpc_url.trim().is_empty() {
        return Err(AppError::Validation("未配置 Aria2 RPC URL".to_string()));
    }
    Ok(Aria2Client::new(
        settings.aria2_rpc_url.clone(),
        settings.aria2_secret.clone(),
        String::new(),
    ))
}

fn download_completed_title_message(task: &Aria2Task) -> (String, String) {
    let file_name = if task.file_name.trim().is_empty() {
        task.gid.as_str()
    } else {
        task.file_name.trim()
    };
    let title = format!("下载完成: {}", file_name);
    let mut parts = vec![format!("文件：{}", file_name)];
    if !task.dir.trim().is_empty() {
        parts.push(format!("目录：{}", task.dir.trim()));
    }
    if task.total_length > 0 {
        parts.push(format!("大小：{}", format_bytes(task.total_length)));
    }
    let message = parts.join("\n");
    (title, message)
}

fn download_completed_meta(task: &Aria2Task) -> HashMap<String, Value> {
    HashMap::from([
        ("gid".to_string(), json!(task.gid)),
        ("file_name".to_string(), json!(task.file_name)),
        ("dir".to_string(), json!(task.dir)),
        ("total_length".to_string(), json!(task.total_length)),
        ("completed_length".to_string(), json!(task.completed_length)),
    ])
}

fn download_completed_dedupe_keys(task: &Aria2Task) -> Vec<String> {
    let mut keys = Vec::with_capacity(2);
    let gid = task.gid.trim();
    if !gid.is_empty() {
        keys.push(format!("gid:{}", gid));
    }
    keys.push(format!(
        "file:{}\n{}\n{}",
        task.file_name.trim(),
        task.dir.trim(),
        task.total_length
    ));
    keys
}

fn completed_download_already_recorded(
    history: &[Notification],
    pushed_downloads: &HashSet<(String, String)>,
    task: &Aria2Task,
) -> bool {
    let (title, message) = download_completed_title_message(task);
    pushed_downloads.contains(&(title.clone(), message.clone()))
        || history.iter().any(|notification| {
            notification_matches_completed_download(notification, task, &title, &message)
        })
}

fn notification_matches_completed_download(
    notification: &Notification,
    task: &Aria2Task,
    title: &str,
    message: &str,
) -> bool {
    if notification.event != PushEvent::DownloadCompleted.as_str() {
        return false;
    }
    if notification.meta.get("gid").and_then(Value::as_str) == Some(task.gid.as_str()) {
        return true;
    }
    if notification.title == title && notification.message == message {
        return true;
    }
    let same_file =
        notification.meta.get("file_name").and_then(Value::as_str) == Some(task.file_name.as_str());
    let same_dir = notification.meta.get("dir").and_then(Value::as_str) == Some(task.dir.as_str());
    let same_size = notification
        .meta
        .get("total_length")
        .and_then(Value::as_u64)
        == Some(task.total_length);
    same_file && same_dir && same_size
}

fn subscription_id_for_download_gid(history: &[Notification], gid: &str) -> Option<String> {
    history
        .iter()
        .filter(|notification| notification.event == "subscription_transferred")
        .find_map(|notification| {
            let downloads = notification.meta.get("sync_downloads")?.as_array()?;
            let matched = downloads
                .iter()
                .any(|item| item.get("gid").and_then(Value::as_str) == Some(gid));
            if !matched {
                return None;
            }
            notification
                .meta
                .get("subscription_id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
}

fn download_completed_gids(history: &[Notification], current_gid: &str) -> HashSet<String> {
    let mut gids = history
        .iter()
        .filter(|notification| notification.event == "download_completed")
        .filter_map(|notification| notification.meta.get("gid").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect::<HashSet<_>>();
    gids.insert(current_gid.to_string());
    gids
}

fn completed_subscription_download_files(
    history: &[Notification],
    subscription_id: &str,
    completed_gids: &HashSet<String>,
) -> Vec<String> {
    let mut files = history
        .iter()
        .filter(|notification| notification.event == "subscription_transferred")
        .filter(|notification| {
            notification
                .meta
                .get("subscription_id")
                .and_then(Value::as_str)
                == Some(subscription_id)
        })
        .filter_map(|notification| notification.meta.get("sync_downloads")?.as_array())
        .flat_map(|downloads| downloads.iter())
        .filter(|item| {
            item.get("gid")
                .and_then(Value::as_str)
                .map(|gid| completed_gids.contains(gid))
                .unwrap_or(false)
        })
        .filter_map(|item| item.get("file_name").and_then(Value::as_str))
        .filter(|file_name| !file_name.trim().is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    files
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0usize;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.2} {}", size, UNITS[unit])
    }
}

fn now_ts() -> i64 {
    unix_now()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Notification;

    fn completed_task() -> Aria2Task {
        Aria2Task {
            gid: "gid-1".to_string(),
            status: "complete".to_string(),
            file_name: "Show.S01E01.mkv".to_string(),
            total_length: 1024,
            completed_length: 1024,
            download_speed: 0,
            upload_speed: 0,
            connections: 0,
            progress: 100.0,
            eta_seconds: None,
            dir: "/downloads/anime".to_string(),
            error_code: String::new(),
            error_message: String::new(),
            files: vec![],
        }
    }

    #[test]
    fn completed_download_history_matches_when_gid_changes() {
        let task = completed_task();
        let history = vec![Notification {
            id: "n1".to_string(),
            level: "success".to_string(),
            event: "download_completed".to_string(),
            title: "下载完成: Show.S01E01.mkv".to_string(),
            message: "文件：Show.S01E01.mkv\n目录：/downloads/anime\n大小：1.00 KB".to_string(),
            meta: HashMap::from([
                ("gid".to_string(), json!("old-gid")),
                ("file_name".to_string(), json!("Show.S01E01.mkv")),
                ("dir".to_string(), json!("/downloads/anime")),
                ("total_length".to_string(), json!(1024u64)),
            ]),
            read: false,
            created_at: 1,
        }];

        assert!(completed_download_already_recorded(
            &history,
            &HashSet::new(),
            &task
        ));
    }

    #[test]
    fn completed_download_history_uses_push_jobs_when_notifications_were_cleared() {
        let task = completed_task();
        let (title, message) = download_completed_title_message(&task);
        let pushed_downloads = HashSet::from([(title, message)]);

        assert!(completed_download_already_recorded(
            &[],
            &pushed_downloads,
            &task
        ));
    }
}
