use std::collections::{HashMap, HashSet, VecDeque};
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
/// 内存去重键上限（每个下载最多 2 个键，约等于最近 1000 个下载）。
/// 超出后按插入顺序淘汰最旧的键，防止长期运行时无界增长。
const MAX_TRACKED_DEDUPE_KEYS: usize = 2_000;

/// 插入有序、带上限的去重键缓存。
#[derive(Default)]
struct DedupeKeyCache {
    order: VecDeque<String>,
    keys: HashSet<String>,
}

impl DedupeKeyCache {
    fn contains(&self, key: &str) -> bool {
        self.keys.contains(key)
    }

    fn insert(&mut self, key: String) {
        if !self.keys.insert(key.clone()) {
            return;
        }
        self.order.push_back(key);
        while self.order.len() > MAX_TRACKED_DEDUPE_KEYS {
            if let Some(oldest) = self.order.pop_front() {
                self.keys.remove(&oldest);
            }
        }
    }

    fn remove(&mut self, key: &str) {
        if self.keys.remove(key) {
            self.order.retain(|existing| existing != key);
        }
    }

    fn snapshot(&self) -> HashSet<String> {
        self.keys.clone()
    }
}

/// 后台监控 Aria2 已停止任务，并在下载完成时发出通知。
pub struct DownloadMonitorService {
    settings_store: Arc<SettingsStore>,
    subscription_store: Arc<SubscriptionStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Arc<JobQueue>,
    notified_completed_downloads: RwLock<DedupeKeyCache>,
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
            notified_completed_downloads: RwLock::new(DedupeKeyCache::default()),
        }
    }

    pub fn start(self: Arc<Self>) {
        // 通知摘要的定时器只存在于内存中；随后台监控一起在启动时恢复
        // 重启前遗留的 digest_pending 通知，避免它们永远不被推送。
        crate::services::notification::recover_digest_pending_on_startup(
            self.settings_store.clone(),
            self.notification_store.clone(),
            Some(self.job_queue.clone()),
        );
        crate::services::push::register_settings_store_for_pruning(&self.settings_store);
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
        let known_keys = self.notified_completed_downloads.read().await.snapshot();
        let pending_tasks = tasks
            .iter()
            .filter(|task| task.status == "complete")
            .filter(|task| !task.gid.trim().is_empty())
            .filter(|task| {
                download_completed_dedupe_keys(task)
                    .iter()
                    .all(|key| !known_keys.contains(key))
            })
            .collect::<Vec<_>>();

        if pending_tasks.is_empty() {
            return;
        }

        let mut claimed = Vec::new();
        {
            let mut known = self.notified_completed_downloads.write().await;
            for task in pending_tasks {
                let keys = download_completed_dedupe_keys(task);
                if keys.iter().any(|key| known.contains(key)) {
                    continue;
                }
                for key in &keys {
                    known.insert(key.clone());
                }
                claimed.push((task.gid.clone(), keys));
            }
        }

        for (gid, keys) in claimed {
            if let Some(task) = tasks.iter().find(|task| task.gid == gid) {
                let already_recorded =
                    completed_download_already_recorded(&history, &pushed_downloads, task);
                if let Err(e) = self.notify_completed_download(task, already_recorded).await {
                    warn!("处理 Aria2 下载完成事件失败 {}: {}", task.gid, e);
                    let mut known = self.notified_completed_downloads.write().await;
                    for key in &keys {
                        known.remove(key);
                    }
                }
            }
        }
    }

    async fn notify_completed_download(
        &self,
        task: &Aria2Task,
        already_recorded: bool,
    ) -> Result<()> {
        // 业务状态必须先于展示通知落盘。即使通知已存在，也仍需重放该步骤，
        // 以便修复此前在通知写入后、订阅更新前发生的瞬时失败。
        self.complete_subscription_for_download(task).await?;
        if already_recorded {
            return Ok(());
        }

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
                subscription_id: None,
                event: PushEvent::DownloadCompleted,
                title,
                message,
                level: PushLevel::Success,
            },
        )
        .await;

        Ok(())
    }

    async fn complete_subscription_for_download(&self, task: &Aria2Task) -> Result<()> {
        let history = self.notification_store.list(true).await;
        let gid = task.gid.trim();
        if gid.is_empty() {
            return Ok(());
        }

        let completed_gids = download_completed_gids(&history, gid);
        let subscriptions = self.subscription_store.list().await;
        let exact_ids = subscriptions
            .iter()
            .filter(|sub| {
                sub.sync_downloads
                    .iter()
                    .any(|record| record.gid.trim() == gid)
            })
            .map(|sub| sub.id.clone())
            .collect::<HashSet<_>>();
        let durable_ids = if exact_ids.is_empty() {
            subscriptions
                .iter()
                .filter(|sub| {
                    sub.sync_downloads
                        .iter()
                        .any(|record| sync_download_matches_by_file(record, task))
                })
                .map(|sub| sub.id.clone())
                .collect::<HashSet<_>>()
        } else {
            exact_ids
        };
        let mut subscription_ids = durable_ids.clone();
        if let Some(legacy_id) = subscription_id_for_download_gid(&history, gid) {
            subscription_ids.insert(legacy_id);
        }
        if subscription_ids.is_empty() {
            return Ok(());
        }

        for subscription_id in subscription_ids {
            let sub = if durable_ids.contains(&subscription_id) {
                let completed_at = now_ts();
                self.subscription_store
                    .update(&subscription_id, |sub| {
                        let has_exact = sub
                            .sync_downloads
                            .iter()
                            .any(|record| record.gid.trim() == gid);
                        for record in &mut sub.sync_downloads {
                            let matches = if has_exact {
                                record.gid.trim() == gid
                            } else {
                                sync_download_matches_by_file(record, task)
                            };
                            if matches && record.completed_at.is_none() {
                                record.completed_at = Some(completed_at);
                            }
                        }
                        sub.updated_at = completed_at;
                    })
                    .await?
                    .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?
            } else {
                let Some(sub) = self.subscription_store.get(&subscription_id).await else {
                    continue;
                };
                sub
            };
            if sub.completed || !sub.sync_download_enabled {
                continue;
            }

            let mut completed_files = sub
                .sync_downloads
                .iter()
                .filter(|record| record.completed_at.is_some())
                .map(|record| record.file_name.clone())
                .filter(|file_name| !file_name.trim().is_empty())
                .collect::<Vec<_>>();
            completed_files.extend(completed_subscription_download_files(
                &history,
                &subscription_id,
                &completed_gids,
            ));
            completed_files.sort();
            completed_files.dedup();
            if !should_mark_completed_from_file_names(&sub, &completed_files) {
                continue;
            }

            self.mark_subscription_completed_after_download(&sub, &completed_files)
                .await?;
        }
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
                subscription_id: Some(updated.id.clone()),
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

fn sync_download_matches_by_file(
    record: &crate::models::SyncDownloadRecord,
    task: &Aria2Task,
) -> bool {
    let record_name = record.file_name.trim();
    let task_name = task.file_name.trim();
    if record_name.is_empty()
        || task_name.is_empty()
        || !record_name.eq_ignore_ascii_case(task_name)
    {
        return false;
    }

    let record_dir = record.download_dir.trim().trim_end_matches('/');
    let task_dir = task.dir.trim().trim_end_matches('/');
    record_dir.is_empty() || task_dir.is_empty() || record_dir == task_dir
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
    fn dedupe_key_cache_evicts_oldest_keys_beyond_cap() {
        let mut cache = DedupeKeyCache::default();
        for index in 0..(MAX_TRACKED_DEDUPE_KEYS + 10) {
            cache.insert(format!("gid:{index}"));
        }
        assert_eq!(cache.keys.len(), MAX_TRACKED_DEDUPE_KEYS);
        assert_eq!(cache.order.len(), MAX_TRACKED_DEDUPE_KEYS);
        assert!(!cache.contains("gid:0"));
        assert!(!cache.contains("gid:9"));
        assert!(cache.contains("gid:10"));
        assert!(cache.contains(&format!("gid:{}", MAX_TRACKED_DEDUPE_KEYS + 9)));

        // 重复插入不产生重复的淘汰顺序条目。
        let newest = format!("gid:{}", MAX_TRACKED_DEDUPE_KEYS + 9);
        cache.insert(newest.clone());
        assert_eq!(cache.order.len(), MAX_TRACKED_DEDUPE_KEYS);
        assert!(cache.contains(&newest));
    }

    #[test]
    fn failed_claim_can_be_removed_for_retry() {
        let mut cache = DedupeKeyCache::default();
        cache.insert("gid:retry".to_string());
        cache.insert("file:retry".to_string());

        cache.remove("gid:retry");
        cache.remove("file:retry");

        assert!(!cache.contains("gid:retry"));
        assert!(!cache.contains("file:retry"));
        assert!(cache.order.is_empty());
    }

    #[tokio::test]
    async fn persisted_download_mapping_completes_without_transfer_notification() {
        use crate::app::AppContext;
        use crate::config::{Config, ServerConfig};
        use crate::models::SyncDownloadRecord;

        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-download-monitor-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let context = AppContext::new(&Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                username: "admin".to_string(),
                password: "test-password".to_string(),
            },
            data_dir: dir.clone(),
        })
        .await
        .unwrap();
        let mut subscription: Subscription = serde_json::from_value(json!({
            "id": "sub-download",
            "title": "Show",
            "url": "https://pan.quark.cn/s/test",
            "created_at": 1,
            "updated_at": 1,
            "last_checked_at": 1
        }))
        .unwrap();
        subscription.media_type = "series".to_string();
        subscription.total_episode_number = Some(1);
        subscription.sync_download_enabled = true;
        subscription.transferred_files = vec!["Show.S01E01.mkv".to_string()];
        subscription.sync_downloads = vec![SyncDownloadRecord {
            gid: "gid-1".to_string(),
            file_name: "Show.S01E01.mkv".to_string(),
            download_dir: "/downloads/anime".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 1,
            completed_at: None,
        }];
        context
            .subscription_store
            .create(subscription)
            .await
            .unwrap();

        // 模拟旧流程已经写入下载完成通知，但订阅状态更新失败；这里没有任何
        // subscription_transferred 通知，业务关联只能来自持久下载记录。
        context
            .notification_store
            .add(Notification {
                id: "existing-download-notification".to_string(),
                level: "success".to_string(),
                event: "download_completed".to_string(),
                title: "下载完成: Show.S01E01.mkv".to_string(),
                message: "already recorded".to_string(),
                meta: HashMap::from([("gid".to_string(), json!("gid-1"))]),
                read: false,
                created_at: 1,
            })
            .await
            .unwrap();

        context
            .download_monitor
            .notify_completed_downloads(&[completed_task()])
            .await;

        let updated = context
            .subscription_store
            .get("sub-download")
            .await
            .unwrap();
        assert!(updated.completed);
        assert_eq!(updated.status, "completed");
        assert!(updated.sync_downloads[0].completed_at.is_some());
        let notifications = context.notification_store.list(true).await;
        assert_eq!(
            notifications
                .iter()
                .filter(|notification| notification.event == "download_completed")
                .count(),
            1
        );

        context.job_queue.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
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
