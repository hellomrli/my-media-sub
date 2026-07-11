use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

use crate::error::{AppError, Result};
use crate::models::{episode_count_for_season, MediaMetadata, Subscription};
use crate::providers::{CloudDriveProviderRegistry, TransferRequest};
use crate::services::notification::add_notification;
use crate::services::push::{
    record_push_message_report_for_notification, PushEvent, PushLevel, PushRetryPolicy, PushService,
};
use crate::services::subscription_progress::reopen_completed_subscription_status;
use crate::services::{MetadataService, SubscriptionTransferService};
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};

use super::model::{
    job_idempotency_key, now, Job, JobErrorClass, JobKind, JobPriority, JobStatus,
    ManualTransferPayload, MetadataScrapePayload, PushDispatchPayload, SubscriptionTransferPayload,
};
use super::reliability::{
    classify_app_error, is_retryable, job_error_class, job_history_retain, retry_delay_seconds,
    CircuitBreakers, JOB_BACKLOG_WARNING_THRESHOLD, JOB_STUCK_TIMEOUT_SECONDS, MAX_AUTO_ATTEMPTS,
};
use super::scheduler::{FairScheduler, JobConcurrencyLimits, RunningJobs};
use super::store::JobStore;

mod manual_transfer;
mod metadata_scrape;
mod push_dispatch;
mod subscription_transfer;

const MAX_SIGNAL_DRAIN_PER_TICK: usize = 512;

pub(crate) struct JobWorker {
    pub(crate) store: Arc<JobStore>,
    pub(crate) sender: mpsc::Sender<String>,
    pub(crate) settings_store: Arc<SettingsStore>,
    pub(crate) subscription_store: Arc<SubscriptionStore>,
    pub(crate) notification_store: Arc<NotificationStore>,
    pub(crate) metadata_service: Arc<MetadataService>,
    pub(crate) transfer_service: Arc<SubscriptionTransferService>,
    pub(crate) receiver: mpsc::Receiver<String>,
}

impl JobWorker {
    pub(crate) async fn run(mut self) {
        info!("后台任务 worker 已启动（优先级公平调度与分层并发已启用）");
        let mut scheduler = FairScheduler::default();
        let mut running = RunningJobs::default();
        let mut tasks = JoinSet::<(Job, JobTaskResult)>::new();
        let mut circuits = CircuitBreakers::default();
        let mut receiver_open = true;
        let mut last_backlog_alert_at = 0_i64;

        loop {
            // 限制单轮信号吸收量，避免持续提交者让调度阶段永远得不到执行机会。
            for _ in 0..MAX_SIGNAL_DRAIN_PER_TICK {
                let Ok(job_id) = self.receiver.try_recv() else {
                    break;
                };
                self.queue_pending_job(&mut scheduler, &job_id).await;
            }

            let settings = self.settings_store.get().await;
            self.check_backlog(&mut last_backlog_alert_at).await;
            let limits = JobConcurrencyLimits::from_settings(&settings);
            if !settings.job_maintenance_mode {
                while let Some(job) = scheduler.pop_next(|job| {
                    running.can_start(job, limits)
                        && circuits.allow(super::scheduler::job_resource(job).class, now())
                }) {
                    match self.claim_job(&job).await {
                        Ok(true) => {}
                        Ok(false) => {
                            circuits.release_probe(super::scheduler::job_resource(&job).class);
                            continue;
                        }
                        Err(error) => {
                            circuits.release_probe(super::scheduler::job_resource(&job).class);
                            error!("认领排队任务 {} 失败: {}", job.id, error);
                            continue;
                        }
                    }
                    running.start(&job);
                    let runner = self.worker_for_job();
                    let task_job_id = job.id.clone();
                    let log_context = crate::observability::LogContext {
                        request_id: job.request_id.clone(),
                        correlation_id: job.correlation_id.clone(),
                        subscription_id: job.subscription_id.clone(),
                        job_id: Some(job.id.clone()),
                    };
                    let job_span = crate::observability::job_span(
                        &log_context,
                        job.kind.as_str(),
                        job.attempt,
                    );
                    tasks.spawn(async move {
                        crate::observability::in_context(log_context, job_span, async move {
                            let job_started = std::time::Instant::now();
                            info!("job execution started");
                            // 内层 task 保留 panic 隔离；外层 task 始终带回 Job，以便准确释放
                            // 全局、类别和订阅资源计数。
                            let execution_span = tracing::Span::current();
                            let execution_context = crate::observability::current_context();
                            let handle = tokio::spawn(async move {
                                crate::observability::in_context(
                                    execution_context,
                                    execution_span,
                                    async move {
                                        tokio::time::timeout(
                                            std::time::Duration::from_secs(
                                                JOB_STUCK_TIMEOUT_SECONDS,
                                            ),
                                            runner.run_job(&task_job_id),
                                        )
                                        .await
                                    },
                                )
                                .await
                            });
                            let outcome = match handle.await {
                                Ok(Ok(result)) => JobTaskResult::Finished(result),
                                Ok(Err(_elapsed)) => JobTaskResult::TimedOut,
                                Err(join_error) => JobTaskResult::Panicked(join_error.to_string()),
                            };
                            let duration = job_started.elapsed();
                            crate::utils::metrics::global_metrics().observe_slow_operation(
                                &format!("job:{}", job.kind.as_str()),
                                duration,
                            );
                            info!(
                                outcome = outcome.as_str(),
                                duration_ms = duration.as_millis(),
                                "job execution finished"
                            );
                            (job, outcome)
                        })
                        .await
                    });
                }
            }

            if tasks.is_empty() {
                if !receiver_open {
                    break;
                }
                if scheduler.is_empty() {
                    match self.receiver.recv().await {
                        Some(job_id) => self.queue_pending_job(&mut scheduler, &job_id).await,
                        None => receiver_open = false,
                    }
                } else {
                    tokio::select! {
                        maybe_job_id = self.receiver.recv() => match maybe_job_id {
                            Some(job_id) => self.queue_pending_job(&mut scheduler, &job_id).await,
                            None => receiver_open = false,
                        },
                        _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
                    }
                }
                continue;
            }

            tokio::select! {
                maybe_job_id = self.receiver.recv(), if receiver_open => {
                    match maybe_job_id {
                        Some(job_id) => self.queue_pending_job(&mut scheduler, &job_id).await,
                        None => receiver_open = false,
                    }
                }
                completed = tasks.join_next() => {
                    match completed {
                        Some(Ok((job, outcome))) => {
                            running.finish(&job);
                            self.handle_task_result(&job.id, outcome).await;
                            self.apply_reliability(&job, &mut circuits).await;
                            if let Err(error) = self.store.archive_completed(job_history_retain()).await {
                                warn!("归档历史任务失败: {}", error);
                            }
                        }
                        Some(Err(join_error)) => {
                            // 包装 task 本身没有业务逻辑，正常情况下不会 panic。
                            error!("任务调度包装器异常退出: {}", join_error);
                        }
                        None => {}
                    }
                }
            }

            if !receiver_open && tasks.is_empty() && scheduler.is_empty() {
                break;
            }
        }
        warn!("后台任务 worker 已停止");
    }

    async fn queue_pending_job(&self, scheduler: &mut FairScheduler, job_id: &str) {
        let Some(job) = self.store.get(job_id).await else {
            warn!("忽略不存在的排队任务: {}", job_id);
            return;
        };
        if job.status == JobStatus::Queued {
            if let Some(due_at) = job.next_attempt_at.filter(|due| *due > now()) {
                let sender = self.sender.clone();
                let id = job.id;
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(
                        due_at.saturating_sub(now()) as u64,
                    ))
                    .await;
                    let _ = sender.send(id).await;
                });
                return;
            }
            scheduler.push(job);
        }
    }

    async fn claim_job(&self, scheduled: &Job) -> Result<bool> {
        Ok(self
            .store
            .update_if(&scheduled.id, |job| {
                // 若 API 在调度快照选中后先修改了优先级，则放弃旧快照；API
                // 发送的唤醒信号会按新优先级重新入队。
                if job.status != JobStatus::Queued || job.priority != scheduled.priority {
                    return false;
                }
                job.status = JobStatus::Running;
                job.message = "已调度，等待后台任务执行".to_string();
                job.started_at = Some(now());
                true
            })
            .await?
            .is_some())
    }

    async fn handle_task_result(&self, job_id: &str, outcome: JobTaskResult) {
        match outcome {
            JobTaskResult::Finished(Ok(())) => {}
            JobTaskResult::Finished(Err(_)) if self.is_canceled(job_id).await => {
                info!("任务 {} 已取消", job_id);
            }
            JobTaskResult::Finished(Err(error)) => {
                error!("任务 {} 执行失败: {}", job_id, error);
                if let Err(mark_error) = self.fail_execution_error(job_id, &error).await {
                    error!("标记失败任务 {} 失败: {}", job_id, mark_error);
                }
            }
            JobTaskResult::Panicked(_join_error) if self.is_canceled(job_id).await => {
                info!("任务 {} 已取消", job_id);
            }
            JobTaskResult::Panicked(join_error) => {
                error!("任务 {} panic: {}", job_id, join_error);
                if let Err(error) = self.fail_panicked_job(job_id, &join_error).await {
                    error!("标记 panic 任务 {} 失败: {}", job_id, error);
                }
            }
            JobTaskResult::TimedOut => {
                error!("任务 {} 超过卡死检测阈值，已终止", job_id);
                let _ = self
                    .store
                    .update_if(job_id, |job| {
                        if job.status != JobStatus::Running {
                            return false;
                        }
                        job.status = JobStatus::Failed;
                        job.progress = 100;
                        job.message = "任务长时间无响应，已终止并等待恢复策略处理".to_string();
                        job.error = Some("job execution timed out".to_string());
                        job.error_class = Some(JobErrorClass::TimedOut);
                        job.finished_at = Some(now());
                        true
                    })
                    .await;
            }
        }
    }

    async fn apply_reliability(&self, scheduled: &Job, circuits: &mut CircuitBreakers) {
        let Some(job) = self.store.get(&scheduled.id).await else {
            return;
        };
        let class = super::scheduler::job_resource(&job).class;
        if job.status == JobStatus::Succeeded {
            circuits.record_success(class);
            let _ = self
                .store
                .update_if(&job.id, |current| {
                    if current.status != JobStatus::Succeeded {
                        return false;
                    }
                    current.error = None;
                    current.error_class = None;
                    current.next_attempt_at = None;
                    true
                })
                .await;
            return;
        }
        let Some(error_class) = job_error_class(&job) else {
            circuits.release_probe(class);
            return;
        };
        circuits.record_failure(class, error_class);
        let _ = self
            .store
            .update_if(&job.id, |current| {
                if current.status != JobStatus::Failed {
                    return false;
                }
                current.error_class = Some(error_class);
                true
            })
            .await;

        if !job.kind.supports_automatic_retry()
            || !is_retryable(error_class)
            || job.attempt >= MAX_AUTO_ATTEMPTS
        {
            return;
        }
        let next_attempt = job.attempt + 1;
        let delay = retry_delay_seconds(&job.id, next_attempt);
        let due_at = now() + delay;
        if self
            .store
            .update_if(&job.id, |current| {
                if current.status != JobStatus::Failed {
                    return false;
                }
                current.status = JobStatus::Queued;
                current.progress = 0;
                current.attempt = next_attempt;
                current.next_attempt_at = Some(due_at);
                current.started_at = None;
                current.finished_at = None;
                current.message = format!("第 {} 次执行将在 {} 秒后重试", next_attempt, delay);
                true
            })
            .await
            .ok()
            .flatten()
            .is_some()
        {
            let sender = self.sender.clone();
            let id = job.id;
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(delay as u64)).await;
                let _ = sender.send(id).await;
            });
        }
    }

    async fn check_backlog(&self, last_alert_at: &mut i64) {
        let queued = self
            .store
            .list()
            .await
            .into_iter()
            .filter(|job| job.status == JobStatus::Queued)
            .count();
        if queued < JOB_BACKLOG_WARNING_THRESHOLD || now() - *last_alert_at < 3600 {
            return;
        }
        *last_alert_at = now();
        let _ = add_notification(
            &self.notification_store,
            "warning",
            "job_queue_backlog",
            "后台任务队列积压",
            format!("当前有 {queued} 个任务等待执行，请检查维护模式、并发限制和外部服务状态"),
            HashMap::from([
                ("queued".to_string(), json!(queued)),
                (
                    "threshold".to_string(),
                    json!(JOB_BACKLOG_WARNING_THRESHOLD),
                ),
            ]),
        )
        .await;
    }

    fn worker_for_job(&self) -> Self {
        let (_unused_sender, receiver) = mpsc::channel(1);
        Self {
            store: self.store.clone(),
            sender: self.sender.clone(),
            settings_store: self.settings_store.clone(),
            subscription_store: self.subscription_store.clone(),
            notification_store: self.notification_store.clone(),
            metadata_service: self.metadata_service.clone(),
            transfer_service: self.transfer_service.clone(),
            receiver,
        }
    }

    async fn fail_panicked_job(&self, job_id: &str, join_error: &str) -> Result<()> {
        self.store
            .update_if(job_id, |job| {
                if job.status != JobStatus::Running {
                    return false;
                }
                job.status = JobStatus::Failed;
                job.progress = 100;
                job.message = "后台任务异常退出，可重试".to_string();
                job.error = Some(format!("后台任务 panic: {}", join_error));
                job.finished_at = Some(now());
                true
            })
            .await?;
        Ok(())
    }

    async fn fail_execution_error(&self, job_id: &str, error: &AppError) -> Result<()> {
        let error_class = classify_app_error(error);
        self.store
            .update_if(job_id, |job| {
                if job.status != JobStatus::Running {
                    return false;
                }
                job.status = JobStatus::Failed;
                job.progress = 100;
                job.message = "后台任务执行失败，可重试".to_string();
                job.error = Some(error.to_string());
                job.error_class = Some(error_class);
                job.finished_at = Some(now());
                true
            })
            .await?;
        Ok(())
    }

    async fn run_job(&self, job_id: &str) -> Result<()> {
        let Some(job) = self.store.get(job_id).await else {
            warn!("任务不存在: {}", job_id);
            return Ok(());
        };

        if job.status != JobStatus::Running {
            info!("跳过未被调度器认领的任务 {}: {:?}", job_id, job.status);
            return Ok(());
        }

        match job.kind {
            JobKind::ManualTransfer => {
                let payload: ManualTransferPayload = serde_json::from_value(job.payload)?;
                self.run_manual_transfer(job_id, payload).await
            }
            JobKind::SubscriptionTransfer => {
                let payload: SubscriptionTransferPayload = serde_json::from_value(job.payload)?;
                self.run_subscription_transfer(job_id, payload).await
            }
            JobKind::MetadataScrape => {
                let payload: MetadataScrapePayload = serde_json::from_value(job.payload)?;
                self.run_metadata_scrape(job_id, payload).await
            }
            JobKind::PushDispatch => {
                let payload: PushDispatchPayload = serde_json::from_value(job.payload)?;
                self.run_push_dispatch(job_id, payload).await
            }
        }
    }

    async fn update_running(&self, job_id: &str, progress: u8, message: &str) -> Result<()> {
        self.store
            .try_update(job_id, |job| {
                if job.status == JobStatus::Canceled {
                    return Err(AppError::Validation("任务已取消".to_string()));
                }
                job.status = JobStatus::Running;
                job.progress = progress;
                job.message = message.to_string();
                if job.started_at.is_none() {
                    job.started_at = Some(now());
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    async fn is_canceled(&self, job_id: &str) -> bool {
        self.store
            .get(job_id)
            .await
            .is_some_and(|job| job.status == JobStatus::Canceled)
    }

    async fn complete_if_active(
        &self,
        job_id: &str,
        updater: impl FnOnce(&mut Job),
    ) -> Result<bool> {
        let mut completed = false;
        self.store
            .try_update(job_id, |job| {
                if job.status == JobStatus::Canceled {
                    return Ok(());
                }
                updater(job);
                completed = true;
                Ok(())
            })
            .await?;
        Ok(completed)
    }

    async fn add_transfer_notification(
        &self,
        level: &str,
        event: &str,
        title: &str,
        message: &str,
        meta: HashMap<String, serde_json::Value>,
    ) {
        if let Err(e) =
            add_notification(&self.notification_store, level, event, title, message, meta).await
        {
            warn!("写入转存历史失败: {}", e);
        }
    }
}

enum JobTaskResult {
    Finished(Result<()>),
    Panicked(String),
    TimedOut,
}

impl JobTaskResult {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Finished(Ok(())) => "succeeded",
            Self::Finished(Err(_)) => "failed",
            Self::Panicked(_) => "panicked",
            Self::TimedOut => "timed_out",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        MediaMetadata, MediaMetadataSeason, MetadataProvider, Subscription, TransferRules,
    };
    use tokio::sync::mpsc;

    fn test_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "my-media-sub-worker-{}-{}.json",
            name,
            uuid::Uuid::new_v4()
        ))
    }

    fn test_subscription() -> Subscription {
        Subscription {
            id: "sub".to_string(),
            title: "Show".to_string(),
            source_title: String::new(),
            media_type: "series".to_string(),
            season: 1,
            start_episode_number: None,
            current_episode_number: 178,
            total_episode_number: Some(178),
            source_group: String::new(),
            tags: vec![],
            metadata: None,
            manual_schedule: None,
            cloud_type: "quark".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: String::new(),
            known_files: vec![],
            known_file_keys: vec![],
            known_episodes: vec![177, 178],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            last_probe: None,
            last_plan_summary: String::new(),
            notify_only: false,
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            strm_enabled: false,
            enabled: true,
            completed: true,
            rules: TransferRules::default(),
            rule_preset_id: String::new(),
            created_at: 1,
            updated_at: 1,
            last_checked_at: 1,
            last_new_files: vec![],
            last_new_episodes: vec![],
            last_check_summary: String::new(),
            check_history: vec![],
            status: "completed".to_string(),
            invalid_since: Some(1),
            last_error: "completed".to_string(),
            rule_summary: String::new(),
            source_candidates: vec![],
            last_source_search_time: None,
            previous_share_links: vec![],
            source_failure_count: 0,
            last_source_switch_at: None,
            source_switch_history: vec![],
        }
    }

    fn metadata_with_episode_count(count: i32) -> MediaMetadata {
        MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "1".to_string(),
            title: "Show".to_string(),
            original_title: String::new(),
            media_type: "series".to_string(),
            overview: String::new(),
            poster_url: None,
            backdrop_url: None,
            release_date: None,
            vote_average: None,
            number_of_episodes: Some(count),
            number_of_seasons: Some(1),
            seasons: vec![MediaMetadataSeason {
                season_number: 1,
                episode_count: Some(count),
                name: "Season 1".to_string(),
                air_date: None,
                poster_url: None,
            }],
            next_episode_to_air: None,
            episodes: vec![],
        }
    }

    fn make_worker(
        subscription_store: Arc<SubscriptionStore>,
    ) -> (
        JobWorker,
        std::path::PathBuf,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let settings_path = test_path("settings");
        let notifications_path = test_path("notifications");
        let jobs_path = test_path("jobs");
        let settings_store = Arc::new(SettingsStore::new(&settings_path));
        let notification_store = Arc::new(NotificationStore::new(&notifications_path));
        let job_store = Arc::new(JobStore::new(&jobs_path));
        let metadata_service = Arc::new(MetadataService::new());
        let transfer_service = Arc::new(SubscriptionTransferService::new(
            subscription_store.clone(),
            settings_store.clone(),
            notification_store.clone(),
        ));
        let (sender, receiver) = mpsc::channel(1);

        (
            JobWorker {
                store: job_store,
                sender,
                settings_store,
                subscription_store,
                notification_store,
                metadata_service,
                transfer_service,
                receiver,
            },
            settings_path,
            notifications_path,
            jobs_path,
        )
    }

    #[tokio::test]
    async fn apply_subscription_metadata_reopens_completed_subscription_when_total_increases() {
        let subscriptions_path = test_path("subscriptions");
        let subscription_store = Arc::new(SubscriptionStore::new(&subscriptions_path));
        subscription_store
            .create(test_subscription())
            .await
            .unwrap();

        let (worker, settings_path, notifications_path, jobs_path) =
            make_worker(subscription_store.clone());
        worker
            .apply_subscription_metadata("sub", metadata_with_episode_count(190))
            .await
            .unwrap();

        let updated = subscription_store.get("sub").await.unwrap();
        assert_eq!(updated.total_episode_number, Some(190));
        assert!(!updated.completed);
        assert_eq!(updated.status, "active");
        assert_eq!(updated.invalid_since, None);
        assert!(updated.last_error.is_empty());

        let _ = std::fs::remove_file(subscriptions_path);
        let _ = std::fs::remove_file(settings_path);
        let _ = std::fs::remove_file(notifications_path);
        let _ = std::fs::remove_file(jobs_path);
    }
}
