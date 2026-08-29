use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::clients::aria2::Aria2Task;
use crate::clients::Aria2Client;
use crate::error::{AppError, Result};
use crate::jobs::JobQueue;
use crate::models::{MediaMetadata, Notification, Settings, Subscription, SyncDownloadRecord};
use crate::services::notification::{
    add_notification, dispatch_push_event_for_notification, PushDispatchRequest,
};
use crate::services::push::{PushEvent, PushLevel};
use crate::services::subscription_progress::{
    completion_target_episode, should_mark_completed_from_file_names,
};
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};
use crate::utils::format_bytes;
use crate::utils::unix_now;

const MONITOR_INTERVAL: Duration = Duration::from_secs(15);
/// 后台只扫描最近的完成记录，避免每 15 秒重复拉取完整的 1000 条 WebUI 历史。
const STOPPED_LIMIT: u64 = 50;
/// 内存去重键上限（每个下载最多 2 个键，约等于最近 1000 个下载）。
/// 超出后按插入顺序淘汰最旧的键，防止长期运行时无界增长。
const MAX_TRACKED_DEDUPE_KEYS: usize = 2_000;

/// 单个下载事件在本轮扫描中的处理结果。
enum CompletionOutcome {
    /// 已终结：通知已发送、已记录或判定为无需通知，去重 claim 应保留。
    Handled,
    /// 批次未结算：释放去重 claim，等批次结算后的扫描重新处理。
    Deferred,
}

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

/// 同一次提交的 Aria2 下载批次（按订阅与提交时间聚合）。
#[derive(Clone)]
struct DownloadBatch {
    subscription_id: String,
    title: String,
    poster_url: Option<String>,
    /// 订阅的媒体类型（series/anime/movie 等），用于媒体库元数据落盘布局。
    media_type: String,
    /// 订阅匹配到的 TMDB 元数据，用于下载完成后写入海报/NFO 文件。
    metadata: Option<MediaMetadata>,
    records: Vec<SyncDownloadRecord>,
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
        let tasks = aria2
            .list_tasks(stopped_limit.clamp(1, STOPPED_LIMIT))
            .await?;
        self.notify_completed_downloads(&tasks.stopped).await;
        Ok(())
    }

    pub async fn notify_completed_downloads(&self, tasks: &[Aria2Task]) {
        let known_keys = self.notified_completed_downloads.read().await.snapshot();
        let pending_tasks = tasks
            .iter()
            .filter(|task| matches!(task.status.as_str(), "complete" | "error"))
            .filter(|task| !task.gid.trim().is_empty())
            .filter(|task| {
                download_dedupe_keys(task)
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
                let keys = download_dedupe_keys(task);
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
                // 每个任务用最新快照做去重：同一轮扫描中前一个任务刚写入的
                // 合并通知，必须对后续任务可见，否则同批会各自补发一条。
                let history = self.notification_store.list(true).await;
                let result = if task.status == "error" {
                    let pushed_failures = self
                        .job_queue
                        .successful_push_dispatch_messages(PushEvent::DownloadFailed.as_str())
                        .await;
                    self.notify_failed_download(task, &history, &pushed_failures)
                        .await
                        .map(|_| CompletionOutcome::Handled)
                } else {
                    let pushed_downloads = self
                        .job_queue
                        .successful_push_dispatch_messages(PushEvent::DownloadCompleted.as_str())
                        .await;
                    self.notify_completed_download(task, tasks, &history, &pushed_downloads)
                        .await
                };
                match result {
                    Ok(CompletionOutcome::Handled) => {}
                    Ok(CompletionOutcome::Deferred) => {
                        // 批次尚未结算（同批其他文件仍在下载）。必须释放本轮
                        // claim，否则该任务会被去重缓存永久挡住：等失败通知
                        // 让批次结算后，没有任何路径再回头发送合并完成通知。
                        // 释放后下一轮扫描会重试，直到批次结算或任务滑出
                        // stopped 窗口；重放的业务状态更新本身是幂等的。
                        debug!("下载批次未结算，稍后重试合并通知: {}", task.gid);
                        let mut known = self.notified_completed_downloads.write().await;
                        for key in &keys {
                            known.remove(key);
                        }
                    }
                    Err(e) => {
                        warn!("处理 Aria2 下载事件失败 {}: {}", task.gid, e);
                        let mut known = self.notified_completed_downloads.write().await;
                        for key in &keys {
                            known.remove(key);
                        }
                    }
                }
            }
        }
    }

    async fn notify_completed_download(
        &self,
        task: &Aria2Task,
        tasks: &[Aria2Task],
        history: &[Notification],
        pushed_downloads: &HashSet<(String, String)>,
    ) -> Result<CompletionOutcome> {
        // 业务状态必须先于展示通知落盘。即使通知已存在，也仍需重放该步骤，
        // 以便修复此前在通知写入后、订阅更新前发生的瞬时失败。
        self.complete_subscription_for_download(task).await?;

        let batch = self.download_batch_for_task(task).await;
        if let Some(batch) = &batch {
            // 下载完成后按 TMDB 元数据写海报/NFO 到本地下载目录。放在通知
            // 分叉之前：合并批次未结算、已记录、重启回放等路径都不会漏写，
            // 且幂等跳过让重复处理（如重启后重新扫描）自动变成免费重试。
            if let Some(metadata) = &batch.metadata {
                let settings = self.settings_store.get().await;
                if let Err(error) =
                    crate::services::media_metadata_files::write_media_metadata_files(
                        &settings,
                        metadata,
                        &batch.media_type,
                        &task.dir,
                    )
                    .await
                {
                    warn!("写入媒体库元数据文件失败（GID {}）: {}", task.gid, error);
                }
            }
            if batch.records.len() > 1 {
                let failed_gids = failed_download_gids(history);
                if !batch_records_settled(&batch.records, &failed_gids) {
                    // 同一次发现的其他文件仍在下载，等全部完成再合并发送。
                    // 返回 Deferred 让调用方释放本轮去重 claim，批次结算后
                    // 的扫描才能重新进入这里发送合并通知。
                    return Ok(CompletionOutcome::Deferred);
                }
                if merged_download_already_recorded(history, pushed_downloads, batch, tasks) {
                    return Ok(CompletionOutcome::Handled);
                }
                return self
                    .notify_batch_completed(batch.clone(), tasks)
                    .await
                    .map(|_| CompletionOutcome::Handled);
            }
        }

        if completed_download_already_recorded(history, pushed_downloads, task) {
            return Ok(CompletionOutcome::Handled);
        }

        let poster_url = batch.as_ref().and_then(|batch| batch.poster_url.clone());
        let (title, message) = download_completed_title_message(task);
        let meta = download_completed_meta(task, poster_url);
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

        Ok(CompletionOutcome::Handled)
    }

    async fn notify_batch_completed(
        &self,
        batch: DownloadBatch,
        tasks: &[Aria2Task],
    ) -> Result<()> {
        let (title, message, meta) = merged_download_message(&batch, tasks);
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
                subscription_id: Some(batch.subscription_id.clone()),
                event: PushEvent::DownloadCompleted,
                title,
                message,
                level: PushLevel::Success,
            },
        )
        .await;

        Ok(())
    }

    async fn notify_failed_download(
        &self,
        task: &Aria2Task,
        history: &[Notification],
        pushed_failures: &HashSet<(String, String)>,
    ) -> Result<()> {
        if failed_download_already_recorded(history, pushed_failures, task) {
            return Ok(());
        }

        let poster_url = self
            .download_batch_for_task(task)
            .await
            .and_then(|batch| batch.poster_url);
        let (title, message) = download_failed_title_message(task);
        let meta = download_failed_meta(task, poster_url);
        let notification = add_notification(
            &self.notification_store,
            "error",
            PushEvent::DownloadFailed.as_str(),
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
                event: PushEvent::DownloadFailed,
                title,
                message,
                level: PushLevel::Error,
            },
        )
        .await;

        Ok(())
    }

    /// 找出任务所属的订阅下载批次；未关联订阅时返回 `None`。
    async fn download_batch_for_task(&self, task: &Aria2Task) -> Option<DownloadBatch> {
        let gid = task.gid.trim();
        let subscriptions = self.subscription_store.list().await;
        let subscription = subscriptions
            .iter()
            .find(|sub| {
                sub.sync_downloads
                    .iter()
                    .any(|record| record.gid.trim() == gid)
            })
            .or_else(|| {
                subscriptions.iter().find(|sub| {
                    sub.sync_downloads
                        .iter()
                        .any(|record| sync_download_matches_by_file(record, task))
                })
            })?;
        let record = subscription.sync_downloads.iter().find(|record| {
            record.gid.trim() == gid || sync_download_matches_by_file(record, task)
        })?;
        if record.submitted_at <= 0 {
            return None;
        }
        let submitted_at = record.submitted_at;
        let records = subscription
            .sync_downloads
            .iter()
            .filter(|candidate| candidate.submitted_at == submitted_at)
            .cloned()
            .collect();
        Some(DownloadBatch {
            subscription_id: subscription.id.clone(),
            title: subscription.title.clone(),
            poster_url: subscription
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.poster_url.clone()),
            media_type: subscription.media_type.clone(),
            metadata: subscription.metadata.clone(),
            records,
        })
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

pub(crate) fn download_completed_title_message(task: &Aria2Task) -> (String, String) {
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

fn download_completed_meta(task: &Aria2Task, poster_url: Option<String>) -> HashMap<String, Value> {
    let mut meta = HashMap::from([
        ("gid".to_string(), json!(task.gid)),
        ("file_name".to_string(), json!(task.file_name)),
        ("dir".to_string(), json!(task.dir)),
        ("total_length".to_string(), json!(task.total_length)),
        ("completed_length".to_string(), json!(task.completed_length)),
    ]);
    if let Some(poster_url) = poster_url {
        meta.insert("poster_url".to_string(), json!(poster_url));
    }
    meta
}

fn download_dedupe_keys(task: &Aria2Task) -> Vec<String> {
    let failed = task.status == "error";
    let mut keys = Vec::with_capacity(2);
    let gid = task.gid.trim();
    if !gid.is_empty() {
        keys.push(if failed {
            format!("failed:gid:{}", gid)
        } else {
            format!("gid:{}", gid)
        });
    }
    keys.push(if failed {
        format!(
            "failed:file:{}\n{}\n{}",
            task.file_name.trim(),
            task.dir.trim(),
            task.total_length
        )
    } else {
        format!(
            "file:{}\n{}\n{}",
            task.file_name.trim(),
            task.dir.trim(),
            task.total_length
        )
    });
    keys
}

fn failed_download_gids(history: &[Notification]) -> HashSet<String> {
    history
        .iter()
        .filter(|notification| notification.event == PushEvent::DownloadFailed.as_str())
        .filter_map(|notification| notification.meta.get("gid").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect()
}

fn batch_records_settled(records: &[SyncDownloadRecord], failed_gids: &HashSet<String>) -> bool {
    records
        .iter()
        .all(|record| record.completed_at.is_some() || failed_gids.contains(record.gid.trim()))
}

fn merged_download_message(
    batch: &DownloadBatch,
    tasks: &[Aria2Task],
) -> (String, String, HashMap<String, Value>) {
    let completed_records = batch
        .records
        .iter()
        .filter(|record| record.completed_at.is_some())
        .collect::<Vec<_>>();
    let title = format!(
        "下载完成: {}（{} 个文件）",
        batch.title,
        completed_records.len()
    );
    let task_size = |gid: &str| {
        tasks
            .iter()
            .find(|task| task.gid == gid)
            .map(|task| task.total_length)
    };
    let mut parts = Vec::new();
    for record in &completed_records {
        let file_name = record.file_name.trim();
        if file_name.is_empty() {
            continue;
        }
        match task_size(&record.gid).filter(|size| *size > 0) {
            Some(size) => parts.push(format!("• {}（{}）", file_name, format_bytes(size))),
            None => parts.push(format!("• {}", file_name)),
        }
    }
    let message = parts.join("\n");
    let gids = completed_records
        .iter()
        .map(|record| json!(record.gid))
        .collect::<Vec<_>>();
    let files = completed_records
        .iter()
        .map(|record| {
            json!({
                "gid": record.gid,
                "file_name": record.file_name,
                "dir": record.download_dir,
            })
        })
        .collect::<Vec<_>>();
    let mut meta = HashMap::from([
        ("subscription_id".to_string(), json!(batch.subscription_id)),
        ("subscription_title".to_string(), json!(batch.title)),
        (
            "batch_submitted_at".to_string(),
            json!(completed_records
                .first()
                .map(|record| record.submitted_at)
                .unwrap_or(0)),
        ),
        ("file_count".to_string(), json!(completed_records.len())),
        ("gids".to_string(), Value::Array(gids)),
        ("files".to_string(), Value::Array(files)),
    ]);
    if let Some(poster_url) = &batch.poster_url {
        meta.insert("poster_url".to_string(), json!(poster_url));
    }
    if let Some(first) = completed_records.first() {
        meta.insert("gid".to_string(), json!(first.gid));
        meta.insert("file_name".to_string(), json!(first.file_name));
    }
    (title, message, meta)
}

fn merged_download_already_recorded(
    history: &[Notification],
    pushed_downloads: &HashSet<(String, String)>,
    batch: &DownloadBatch,
    tasks: &[Aria2Task],
) -> bool {
    let (title, message, _) = merged_download_message(batch, tasks);
    if pushed_downloads.contains(&(title.clone(), message.clone())) {
        return true;
    }
    let batch_gids = batch
        .records
        .iter()
        .map(|record| record.gid.as_str())
        .collect::<HashSet<_>>();
    if history.iter().any(|notification| {
        if notification.event != PushEvent::DownloadCompleted.as_str() {
            return false;
        }
        if notification.title == title && notification.message == message {
            return true;
        }
        let Some(gids) = notification.meta.get("gids").and_then(Value::as_array) else {
            return false;
        };
        notification
            .meta
            .get("subscription_id")
            .and_then(Value::as_str)
            == Some(batch.subscription_id.as_str())
            && gids
                .iter()
                .any(|gid| gid.as_str().is_some_and(|gid| batch_gids.contains(gid)))
    }) {
        return true;
    }
    // 升级前的旧版本按文件逐条发送通知（meta 只有单个 gid，没有 gids 数组）。
    // 若批次内每个文件都已被单独通知过，说明这批已经通知完毕，不应合并补发。
    batch_records_individually_notified(history, pushed_downloads, batch)
}

/// 批次内每个文件是否都已有各自的「下载完成」记录（历史通知或已成功推送）。
/// 用于识别 2.4.0 之前逐文件通知的遗留历史，避免升级重启后整批合并补发。
fn batch_records_individually_notified(
    history: &[Notification],
    pushed_downloads: &HashSet<(String, String)>,
    batch: &DownloadBatch,
) -> bool {
    batch.records.iter().all(|record| {
        let gid = record.gid.trim();
        let covered_in_history = history.iter().any(|notification| {
            notification.event == PushEvent::DownloadCompleted.as_str()
                && (notification.meta.get("gid").and_then(Value::as_str) == Some(gid)
                    || notification
                        .meta
                        .get("gids")
                        .and_then(Value::as_array)
                        .is_some_and(|gids| {
                            gids.iter().any(|candidate| candidate.as_str() == Some(gid))
                        }))
        });
        if covered_in_history {
            return true;
        }
        // 通知历史被清空时，回退到已成功推送的逐文件消息（目录行之后可能
        // 还有旧版本附加的“大小”行，只要求重建的消息是推送消息的前缀）。
        let file_name = record.file_name.trim();
        if file_name.is_empty() {
            return false;
        }
        let title = format!("下载完成: {}", file_name);
        let mut parts = vec![format!("文件：{}", file_name)];
        let dir = record.download_dir.trim();
        if !dir.is_empty() {
            parts.push(format!("目录：{}", dir));
        }
        let message = parts.join("\n");
        pushed_downloads
            .iter()
            .any(|(pushed_title, pushed_message)| {
                pushed_title == &title && pushed_message.starts_with(&message)
            })
    })
}

fn download_failed_title_message(task: &Aria2Task) -> (String, String) {
    let file_name = if task.file_name.trim().is_empty() {
        task.gid.as_str()
    } else {
        task.file_name.trim()
    };
    let title = format!("下载失败: {}", file_name);
    let mut parts = vec![format!("文件：{}", file_name)];
    if !task.dir.trim().is_empty() {
        parts.push(format!("目录：{}", task.dir.trim()));
    }
    let error = if !task.error_message.trim().is_empty() {
        task.error_message.trim().to_string()
    } else if !task.error_code.trim().is_empty() {
        format!("错误码 {}", task.error_code.trim())
    } else {
        "Aria2 任务失败".to_string()
    };
    parts.push(format!("错误：{}", error));
    (title, parts.join("\n"))
}

fn download_failed_meta(task: &Aria2Task, poster_url: Option<String>) -> HashMap<String, Value> {
    let mut meta = HashMap::from([
        ("gid".to_string(), json!(task.gid)),
        ("file_name".to_string(), json!(task.file_name)),
        ("dir".to_string(), json!(task.dir)),
        ("error_code".to_string(), json!(task.error_code)),
        ("error_message".to_string(), json!(task.error_message)),
    ]);
    if let Some(poster_url) = poster_url {
        meta.insert("poster_url".to_string(), json!(poster_url));
    }
    meta
}

fn failed_download_already_recorded(
    history: &[Notification],
    pushed_failures: &HashSet<(String, String)>,
    task: &Aria2Task,
) -> bool {
    let (title, message) = download_failed_title_message(task);
    if pushed_failures.contains(&(title.clone(), message.clone())) {
        return true;
    }
    history.iter().any(|notification| {
        if notification.event != PushEvent::DownloadFailed.as_str() {
            return false;
        }
        if notification.title == title && notification.message == message {
            return true;
        }
        let same_gid =
            notification.meta.get("gid").and_then(Value::as_str) == Some(task.gid.as_str());
        let same_file = notification.meta.get("file_name").and_then(Value::as_str)
            == Some(task.file_name.as_str());
        let same_dir =
            notification.meta.get("dir").and_then(Value::as_str) == Some(task.dir.as_str());
        same_gid || (same_file && same_dir)
    })
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

pub(crate) fn completed_download_already_recorded(
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
    if let Some(gids) = notification.meta.get("gids").and_then(Value::as_array) {
        if gids
            .iter()
            .any(|gid| gid.as_str() == Some(task.gid.as_str()))
        {
            return true;
        }
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

pub(crate) fn subscription_id_for_download_gid(
    history: &[Notification],
    gid: &str,
) -> Option<String> {
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
    for notification in history
        .iter()
        .filter(|notification| notification.event == "download_completed")
    {
        if let Some(merged_gids) = notification.meta.get("gids").and_then(Value::as_array) {
            for gid in merged_gids.iter().filter_map(Value::as_str) {
                gids.insert(gid.to_string());
            }
        }
    }
    gids.insert(current_gid.to_string());
    gids
}

pub(crate) fn completed_subscription_download_files(
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

    fn task_with(gid: &str, file_name: &str, status: &str) -> Aria2Task {
        let failed = status == "error";
        Aria2Task {
            gid: gid.to_string(),
            status: status.to_string(),
            file_name: file_name.to_string(),
            total_length: 1024,
            completed_length: if failed { 0 } else { 1024 },
            download_speed: 0,
            upload_speed: 0,
            connections: 0,
            progress: if failed { 0.0 } else { 100.0 },
            eta_seconds: None,
            dir: "/downloads/anime".to_string(),
            error_code: if failed {
                "1".to_string()
            } else {
                String::new()
            },
            error_message: if failed {
                "下载地址失效".to_string()
            } else {
                String::new()
            },
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

    #[tokio::test]
    async fn batch_downloads_merge_into_one_notification_after_all_complete() {
        use crate::app::AppContext;
        use crate::config::{Config, ServerConfig};
        use crate::models::{MediaMetadata, MetadataProvider, SyncDownloadRecord};

        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-download-batch-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let context = AppContext::new(&Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
            },
            data_dir: dir.clone(),
        })
        .await
        .unwrap();
        let mut subscription: Subscription = serde_json::from_value(json!({
            "id": "sub-batch",
            "title": "Show",
            "url": "https://pan.quark.cn/s/test",
            "created_at": 1,
            "updated_at": 1,
            "last_checked_at": 1
        }))
        .unwrap();
        subscription.media_type = "series".to_string();
        subscription.sync_download_enabled = true;
        subscription.metadata = Some(MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "1".to_string(),
            title: "Show".to_string(),
            original_title: String::new(),
            media_type: "series".to_string(),
            overview: String::new(),
            poster_url: Some("https://image.tmdb.org/t/p/w500/poster.jpg".to_string()),
            backdrop_url: None,
            release_date: None,
            vote_average: None,
            number_of_episodes: None,
            number_of_seasons: None,
            seasons: vec![],
            next_episode_to_air: None,
            episodes: vec![],
        });
        subscription.sync_downloads = vec![
            SyncDownloadRecord {
                gid: "gid-1".to_string(),
                file_name: "Show.S01E01.mkv".to_string(),
                download_dir: "/downloads/anime".to_string(),
                target_dir: "/series/Show/Season 1".to_string(),
                submitted_at: 100,
                completed_at: None,
            },
            SyncDownloadRecord {
                gid: "gid-2".to_string(),
                file_name: "Show.S01E02.mkv".to_string(),
                download_dir: "/downloads/anime".to_string(),
                target_dir: "/series/Show/Season 1".to_string(),
                submitted_at: 100,
                completed_at: None,
            },
        ];
        context
            .subscription_store
            .create(subscription)
            .await
            .unwrap();

        // 第一批只完成一个文件：不能提前发通知。
        context
            .download_monitor
            .notify_completed_downloads(&[task_with("gid-1", "Show.S01E01.mkv", "complete")])
            .await;
        let notifications = context.notification_store.list(true).await;
        assert_eq!(
            notifications
                .iter()
                .filter(|notification| notification.event == "download_completed")
                .count(),
            0
        );

        // 全部完成后合并为一条通知。
        context
            .download_monitor
            .notify_completed_downloads(&[task_with("gid-2", "Show.S01E02.mkv", "complete")])
            .await;
        let notifications = context.notification_store.list(true).await;
        let completed = notifications
            .iter()
            .filter(|notification| notification.event == "download_completed")
            .collect::<Vec<_>>();
        assert_eq!(completed.len(), 1);
        assert!(completed[0].title.contains("2 个文件"));
        assert!(completed[0].message.contains("Show.S01E01.mkv"));
        assert!(completed[0].message.contains("Show.S01E02.mkv"));
        assert_eq!(
            completed[0]
                .meta
                .get("gids")
                .and_then(Value::as_array)
                .map(|gids| gids.len()),
            Some(2)
        );
        assert_eq!(
            completed[0].meta.get("poster_url").and_then(Value::as_str),
            Some("https://image.tmdb.org/t/p/w500/poster.jpg")
        );

        context.job_queue.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn failed_download_notifies_immediately() {
        use crate::app::AppContext;
        use crate::config::{Config, ServerConfig};
        use crate::models::SyncDownloadRecord;

        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-download-failed-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let context = AppContext::new(&Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
            },
            data_dir: dir.clone(),
        })
        .await
        .unwrap();
        let mut subscription: Subscription = serde_json::from_value(json!({
            "id": "sub-failed",
            "title": "Show",
            "url": "https://pan.quark.cn/s/test",
            "created_at": 1,
            "updated_at": 1,
            "last_checked_at": 1
        }))
        .unwrap();
        subscription.sync_download_enabled = true;
        subscription.sync_downloads = vec![SyncDownloadRecord {
            gid: "gid-1".to_string(),
            file_name: "Show.S01E01.mkv".to_string(),
            download_dir: "/downloads/anime".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 100,
            completed_at: None,
        }];
        context
            .subscription_store
            .create(subscription)
            .await
            .unwrap();

        context
            .download_monitor
            .notify_completed_downloads(&[task_with("gid-1", "Show.S01E01.mkv", "error")])
            .await;

        let notifications = context.notification_store.list(true).await;
        let failures = notifications
            .iter()
            .filter(|notification| notification.event == "download_failed")
            .collect::<Vec<_>>();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].level, "error");
        assert!(failures[0].title.contains("下载失败"));
        assert!(failures[0].message.contains("下载地址失效"));

        context.job_queue.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn failed_download_does_not_block_remaining_batch_success() {
        use crate::app::AppContext;
        use crate::config::{Config, ServerConfig};
        use crate::models::SyncDownloadRecord;

        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-download-batch-failed-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let context = AppContext::new(&Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
            },
            data_dir: dir.clone(),
        })
        .await
        .unwrap();
        let mut subscription: Subscription = serde_json::from_value(json!({
            "id": "sub-batch-failed",
            "title": "Show",
            "url": "https://pan.quark.cn/s/test",
            "created_at": 1,
            "updated_at": 1,
            "last_checked_at": 1
        }))
        .unwrap();
        subscription.sync_download_enabled = true;
        subscription.sync_downloads = vec![
            SyncDownloadRecord {
                gid: "gid-1".to_string(),
                file_name: "Show.S01E01.mkv".to_string(),
                download_dir: "/downloads/anime".to_string(),
                target_dir: "/series/Show/Season 1".to_string(),
                submitted_at: 100,
                completed_at: None,
            },
            SyncDownloadRecord {
                gid: "gid-2".to_string(),
                file_name: "Show.S01E02.mkv".to_string(),
                download_dir: "/downloads/anime".to_string(),
                target_dir: "/series/Show/Season 1".to_string(),
                submitted_at: 100,
                completed_at: None,
            },
        ];
        context
            .subscription_store
            .create(subscription)
            .await
            .unwrap();

        context
            .download_monitor
            .notify_completed_downloads(&[task_with("gid-1", "Show.S01E01.mkv", "error")])
            .await;
        let notifications = context.notification_store.list(true).await;
        assert_eq!(
            notifications
                .iter()
                .filter(|notification| notification.event == "download_failed")
                .count(),
            1
        );
        assert_eq!(
            notifications
                .iter()
                .filter(|notification| notification.event == "download_completed")
                .count(),
            0
        );

        context
            .download_monitor
            .notify_completed_downloads(&[task_with("gid-2", "Show.S01E02.mkv", "complete")])
            .await;
        let notifications = context.notification_store.list(true).await;
        let completed = notifications
            .iter()
            .filter(|notification| notification.event == "download_completed")
            .collect::<Vec<_>>();
        assert_eq!(completed.len(), 1);
        assert!(completed[0].title.contains("1 个文件"));
        assert!(completed[0].message.contains("Show.S01E02.mkv"));
        assert!(!completed[0].message.contains("Show.S01E01.mkv"));

        context.job_queue.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn completed_before_batch_failure_still_sends_merged_notification() {
        // 回归：批次“先完成后失败”时，先到的完成通知在批次未结算时只应
        // 挂起而不应永久持有去重 claim——否则失败通知结算批次后，合并
        // 完成通知永远没有机会再发出。
        use crate::app::AppContext;
        use crate::config::{Config, ServerConfig};
        use crate::models::SyncDownloadRecord;

        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-download-batch-completed-first-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let context = AppContext::new(&Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
            },
            data_dir: dir.clone(),
        })
        .await
        .unwrap();
        let mut subscription: Subscription = serde_json::from_value(json!({
            "id": "sub-batch-completed-first",
            "title": "Show",
            "url": "https://pan.quark.cn/s/test",
            "created_at": 1,
            "updated_at": 1,
            "last_checked_at": 1
        }))
        .unwrap();
        subscription.sync_download_enabled = true;
        subscription.sync_downloads = vec![
            SyncDownloadRecord {
                gid: "gid-1".to_string(),
                file_name: "Show.S01E01.mkv".to_string(),
                download_dir: "/downloads/anime".to_string(),
                target_dir: "/series/Show/Season 1".to_string(),
                submitted_at: 100,
                completed_at: None,
            },
            SyncDownloadRecord {
                gid: "gid-2".to_string(),
                file_name: "Show.S01E02.mkv".to_string(),
                download_dir: "/downloads/anime".to_string(),
                target_dir: "/series/Show/Season 1".to_string(),
                submitted_at: 100,
                completed_at: None,
            },
        ];
        context
            .subscription_store
            .create(subscription)
            .await
            .unwrap();

        // 第一轮：gid-1 完成先到，同批 gid-2 仍在下载 → 批次未结算，无通知。
        context
            .download_monitor
            .notify_completed_downloads(&[task_with("gid-1", "Show.S01E01.mkv", "complete")])
            .await;
        let notifications = context.notification_store.list(true).await;
        assert_eq!(
            notifications
                .iter()
                .filter(|notification| notification.event == "download_completed")
                .count(),
            0
        );
        assert_eq!(
            notifications
                .iter()
                .filter(|notification| notification.event == "download_failed")
                .count(),
            0
        );

        // 第二轮：gid-2 失败 → 立即写失败通知，批次就此结算。
        context
            .download_monitor
            .notify_completed_downloads(&[task_with("gid-2", "Show.S01E02.mkv", "error")])
            .await;
        let notifications = context.notification_store.list(true).await;
        assert_eq!(
            notifications
                .iter()
                .filter(|notification| notification.event == "download_failed")
                .count(),
            1
        );
        assert_eq!(
            notifications
                .iter()
                .filter(|notification| notification.event == "download_completed")
                .count(),
            0
        );

        // 第三轮：gid-1 的完成事件再次被扫描到（claim 已释放）→ 合并通知发出。
        context
            .download_monitor
            .notify_completed_downloads(&[task_with("gid-1", "Show.S01E01.mkv", "complete")])
            .await;
        let notifications = context.notification_store.list(true).await;
        let completed = notifications
            .iter()
            .filter(|notification| notification.event == "download_completed")
            .collect::<Vec<_>>();
        assert_eq!(completed.len(), 1);
        assert!(completed[0].title.contains("1 个文件"));
        assert!(completed[0].message.contains("Show.S01E01.mkv"));
        assert!(!completed[0].message.contains("Show.S01E02.mkv"));

        context.job_queue.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn merged_download_notification_matches_any_batch_gid() {
        let task = completed_task();
        let history = vec![Notification {
            id: "merged".to_string(),
            level: "success".to_string(),
            event: "download_completed".to_string(),
            title: "下载完成: Show（2 个文件）".to_string(),
            message: "• Show.S01E01.mkv\n• Show.S01E02.mkv".to_string(),
            meta: HashMap::from([("gids".to_string(), json!(["gid-1", "gid-2"]))]),
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

    /// 重启恢复场景：记录的完成状态已落盘、内存去重缓存为空，同一批次的
    /// 任务在同一轮扫描中全部出现。即使先处理的任务已发出合并通知，后续
    /// 任务也必须能看见它，整批只能补发一条。
    #[tokio::test]
    async fn settled_batch_in_one_poll_sends_single_merged_notification() {
        use crate::app::AppContext;
        use crate::config::{Config, ServerConfig};
        use crate::models::SyncDownloadRecord;

        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-download-batch-restart-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let context = AppContext::new(&Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
            },
            data_dir: dir.clone(),
        })
        .await
        .unwrap();
        let mut subscription: Subscription = serde_json::from_value(json!({
            "id": "sub-batch-restart",
            "title": "Show",
            "url": "https://pan.quark.cn/s/test",
            "created_at": 1,
            "updated_at": 1,
            "last_checked_at": 1
        }))
        .unwrap();
        subscription.sync_download_enabled = true;
        subscription.completed = true;
        subscription.sync_downloads = vec![
            SyncDownloadRecord {
                gid: "gid-1".to_string(),
                file_name: "Show.S01E01.mkv".to_string(),
                download_dir: "/downloads/anime".to_string(),
                target_dir: "/series/Show/Season 1".to_string(),
                submitted_at: 100,
                completed_at: Some(1),
            },
            SyncDownloadRecord {
                gid: "gid-2".to_string(),
                file_name: "Show.S01E02.mkv".to_string(),
                download_dir: "/downloads/anime".to_string(),
                target_dir: "/series/Show/Season 1".to_string(),
                submitted_at: 100,
                completed_at: Some(1),
            },
        ];
        context
            .subscription_store
            .create(subscription)
            .await
            .unwrap();

        // 两个已完成任务出现在同一轮扫描中。
        context
            .download_monitor
            .notify_completed_downloads(&[
                task_with("gid-1", "Show.S01E01.mkv", "complete"),
                task_with("gid-2", "Show.S01E02.mkv", "complete"),
            ])
            .await;

        let notifications = context.notification_store.list(true).await;
        let completed = notifications
            .iter()
            .filter(|notification| notification.event == "download_completed")
            .collect::<Vec<_>>();
        assert_eq!(completed.len(), 1, "同批只能补发一条合并通知");
        assert!(completed[0].title.contains("2 个文件"));

        context.job_queue.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 本地图片测试服务器，返回请求计数与 URL。
    async fn spawn_image_server() -> (Arc<std::sync::atomic::AtomicUsize>, String) {
        use std::sync::atomic::AtomicUsize;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let body = b"fake-jpeg-bytes";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
            }
        });
        (requests, format!("http://{addr}/poster.jpg"))
    }

    fn completed_task_in_dir(gid: &str, file_name: &str, dir: &str) -> Aria2Task {
        Aria2Task {
            gid: gid.to_string(),
            status: "complete".to_string(),
            file_name: file_name.to_string(),
            total_length: 1024,
            completed_length: 1024,
            download_speed: 0,
            upload_speed: 0,
            connections: 0,
            progress: 100.0,
            eta_seconds: None,
            dir: dir.to_string(),
            error_code: String::new(),
            error_message: String::new(),
            files: vec![],
        }
    }

    async fn metadata_subscription(dir: &str) -> Subscription {
        use crate::models::{MediaMetadata, MediaMetadataSeason, MetadataProvider};

        let mut subscription: Subscription = serde_json::from_value(json!({
            "id": "sub-metadata",
            "title": "Show",
            "url": "https://pan.quark.cn/s/test",
            "created_at": 1,
            "updated_at": 1,
            "last_checked_at": 1
        }))
        .unwrap();
        subscription.media_type = "series".to_string();
        subscription.sync_download_enabled = true;
        subscription.metadata = Some(MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "123".to_string(),
            title: "Show".to_string(),
            original_title: "Original Show".to_string(),
            media_type: "series".to_string(),
            overview: "简介".to_string(),
            poster_url: Some("http://127.0.0.1:1/poster.jpg".to_string()),
            backdrop_url: None,
            release_date: Some("2024-01-01".to_string()),
            vote_average: Some(8.2),
            number_of_episodes: Some(1),
            number_of_seasons: Some(1),
            seasons: vec![MediaMetadataSeason {
                season_number: 1,
                episode_count: Some(1),
                name: "Season 1".to_string(),
                air_date: Some("2024-01-01".to_string()),
                poster_url: Some("http://127.0.0.1:1/poster.jpg".to_string()),
            }],
            next_episode_to_air: None,
            episodes: vec![],
        });
        subscription.sync_downloads = vec![SyncDownloadRecord {
            gid: "gid-1".to_string(),
            file_name: "Show.S01E01.mkv".to_string(),
            download_dir: dir.to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 100,
            completed_at: None,
        }];
        subscription
    }

    /// 完整链路：开关打开时，下载完成会按 TMDB 元数据把 NFO/海报写入本地下载目录。
    #[tokio::test]
    async fn completed_download_writes_media_metadata_files() {
        use crate::app::AppContext;
        use crate::config::{Config, ServerConfig};

        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-download-metadata-{}",
            uuid::Uuid::new_v4()
        ));
        let season_dir = dir.join("Show/Season 1");
        std::fs::create_dir_all(&season_dir).unwrap();
        let (requests, url) = spawn_image_server().await;

        let context = AppContext::new(&Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
            },
            data_dir: dir.clone(),
        })
        .await
        .unwrap();
        context
            .settings_store
            .update(|settings| settings.media_metadata_files_enabled = true)
            .await
            .unwrap();
        let mut subscription = metadata_subscription(season_dir.to_str().unwrap()).await;
        if let Some(metadata) = subscription.metadata.as_mut() {
            metadata.poster_url = Some(url.clone());
            metadata.seasons[0].poster_url = Some(url.clone());
        }
        context
            .subscription_store
            .create(subscription)
            .await
            .unwrap();

        context
            .download_monitor
            .notify_completed_downloads(&[completed_task_in_dir(
                "gid-1",
                "Show.S01E01.mkv",
                season_dir.to_str().unwrap(),
            )])
            .await;

        let show_root = dir.join("Show");
        let tvshow = std::fs::read_to_string(show_root.join("tvshow.nfo")).unwrap();
        assert!(tvshow.contains("<title>Show</title>"));
        assert!(tvshow.contains("<uniqueid type=\"tmdb\">123</uniqueid>"));
        assert!(std::fs::read_to_string(season_dir.join("season.nfo"))
            .unwrap()
            .contains("<seasonnumber>1</seasonnumber>"));
        assert_eq!(
            std::fs::read(show_root.join("poster.jpg")).unwrap(),
            b"fake-jpeg-bytes"
        );
        assert_eq!(
            std::fs::read(season_dir.join("poster.jpg")).unwrap(),
            b"fake-jpeg-bytes"
        );
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 2);

        context.job_queue.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 开关关闭时不写任何媒体库元数据文件。
    #[tokio::test]
    async fn completed_download_skips_metadata_files_when_disabled() {
        use crate::app::AppContext;
        use crate::config::{Config, ServerConfig};

        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-download-metadata-off-{}",
            uuid::Uuid::new_v4()
        ));
        let season_dir = dir.join("Show/Season 1");
        std::fs::create_dir_all(&season_dir).unwrap();
        let (requests, url) = spawn_image_server().await;

        let context = AppContext::new(&Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
            },
            data_dir: dir.clone(),
        })
        .await
        .unwrap();
        let mut subscription = metadata_subscription(season_dir.to_str().unwrap()).await;
        if let Some(metadata) = subscription.metadata.as_mut() {
            metadata.poster_url = Some(url.clone());
            metadata.seasons[0].poster_url = Some(url.clone());
        }
        context
            .subscription_store
            .create(subscription)
            .await
            .unwrap();

        context
            .download_monitor
            .notify_completed_downloads(&[completed_task_in_dir(
                "gid-1",
                "Show.S01E01.mkv",
                season_dir.to_str().unwrap(),
            )])
            .await;

        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(!dir.join("Show/tvshow.nfo").exists());
        assert!(!season_dir.join("poster.jpg").exists());

        context.job_queue.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 重启回放：第二轮重新扫描已完成任务时，通知不再发、文件不再重写，
    /// 但第一轮写入的内容保持不变。
    #[tokio::test]
    async fn replay_after_restart_does_not_rewrite_metadata_files() {
        use crate::app::AppContext;
        use crate::config::{Config, ServerConfig};

        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-download-metadata-replay-{}",
            uuid::Uuid::new_v4()
        ));
        let season_dir = dir.join("Show/Season 1");
        std::fs::create_dir_all(&season_dir).unwrap();
        let (requests, url) = spawn_image_server().await;

        let context = AppContext::new(&Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
            },
            data_dir: dir.clone(),
        })
        .await
        .unwrap();
        context
            .settings_store
            .update(|settings| settings.media_metadata_files_enabled = true)
            .await
            .unwrap();
        let mut subscription = metadata_subscription(season_dir.to_str().unwrap()).await;
        if let Some(metadata) = subscription.metadata.as_mut() {
            metadata.poster_url = Some(url.clone());
            metadata.seasons[0].poster_url = Some(url.clone());
        }
        context
            .subscription_store
            .create(subscription)
            .await
            .unwrap();

        let task = completed_task_in_dir("gid-1", "Show.S01E01.mkv", season_dir.to_str().unwrap());
        context
            .download_monitor
            .notify_completed_downloads(&[task])
            .await;
        let tvshow_path = dir.join("Show/tvshow.nfo");
        let first_mtime = std::fs::metadata(&tvshow_path).unwrap().modified().unwrap();

        // 模拟重启：内存去重缓存清空后再次扫描同一批任务。
        context
            .download_monitor
            .notified_completed_downloads
            .write()
            .await
            .keys
            .clear();
        let task = completed_task_in_dir("gid-1", "Show.S01E01.mkv", season_dir.to_str().unwrap());
        context
            .download_monitor
            .notify_completed_downloads(&[task])
            .await;

        let second_mtime = std::fs::metadata(&tvshow_path).unwrap().modified().unwrap();
        assert_eq!(first_mtime, second_mtime, "幂等跳过时不应重写 NFO");
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 4);
        let notifications = context.notification_store.list(true).await;
        assert_eq!(
            notifications
                .iter()
                .filter(|notification| notification.event == "download_completed")
                .count(),
            1,
            "回放不得重复发送下载完成通知"
        );

        context.job_queue.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 升级场景：旧版本（2.4.0 之前）已按文件逐条发送「下载完成」通知，
    /// meta 只有单个 gid。升级重启后不得把同一批次再次合并补发。
    #[tokio::test]
    async fn legacy_per_file_history_does_not_trigger_merged_resend() {
        use crate::app::AppContext;
        use crate::config::{Config, ServerConfig};
        use crate::models::SyncDownloadRecord;

        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-download-batch-legacy-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let context = AppContext::new(&Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
            },
            data_dir: dir.clone(),
        })
        .await
        .unwrap();
        let mut subscription: Subscription = serde_json::from_value(json!({
            "id": "sub-batch-legacy",
            "title": "Show",
            "url": "https://pan.quark.cn/s/test",
            "created_at": 1,
            "updated_at": 1,
            "last_checked_at": 1
        }))
        .unwrap();
        subscription.sync_download_enabled = true;
        subscription.completed = true;
        subscription.sync_downloads = vec![
            SyncDownloadRecord {
                gid: "gid-1".to_string(),
                file_name: "Show.S01E01.mkv".to_string(),
                download_dir: "/downloads/anime".to_string(),
                target_dir: "/series/Show/Season 1".to_string(),
                submitted_at: 100,
                completed_at: Some(1),
            },
            SyncDownloadRecord {
                gid: "gid-2".to_string(),
                file_name: "Show.S01E02.mkv".to_string(),
                download_dir: "/downloads/anime".to_string(),
                target_dir: "/series/Show/Season 1".to_string(),
                submitted_at: 100,
                completed_at: Some(1),
            },
        ];
        context
            .subscription_store
            .create(subscription)
            .await
            .unwrap();

        // 旧版本遗留的逐文件通知：标题不同、meta 只有单个 gid。
        for (id, gid, file_name) in [
            ("legacy-1", "gid-1", "Show.S01E01.mkv"),
            ("legacy-2", "gid-2", "Show.S01E02.mkv"),
        ] {
            context
                .notification_store
                .add(Notification {
                    id: id.to_string(),
                    level: "success".to_string(),
                    event: "download_completed".to_string(),
                    title: format!("下载完成: {}", file_name),
                    message: format!("文件：{}\n目录：/downloads/anime", file_name),
                    meta: HashMap::from([("gid".to_string(), json!(gid))]),
                    read: false,
                    created_at: 1,
                })
                .await
                .unwrap();
        }

        context
            .download_monitor
            .notify_completed_downloads(&[
                task_with("gid-1", "Show.S01E01.mkv", "complete"),
                task_with("gid-2", "Show.S01E02.mkv", "complete"),
            ])
            .await;

        let notifications = context.notification_store.list(true).await;
        // 除遗留的逐文件通知外，不得再新增任何合并通知。
        let merged = notifications
            .iter()
            .filter(|notification| {
                notification.event == "download_completed" && notification.title.contains("个文件")
            })
            .collect::<Vec<_>>();
        assert_eq!(merged.len(), 0, "已逐文件通知过的批次不得合并补发");

        context.job_queue.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }
}
