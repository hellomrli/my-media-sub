use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::clients::{QuarkSaveClient, QuarkShareProbe};
use crate::error::{AppError, Result};
use crate::models::{episode_count_for_season, MediaMetadata, Subscription};
use crate::services::notification::add_notification;
use crate::services::push::{
    record_push_message_report_for_notification, PushEvent, PushLevel, PushRetryPolicy, PushService,
};
use crate::services::subscription_progress::reopen_completed_subscription_status;
use crate::services::{MetadataService, SubscriptionTransferService};
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};

use super::model::{
    job_idempotency_key, now, Job, JobKind, JobStatus, ManualTransferPayload,
    MetadataScrapePayload, PushDispatchPayload, SubscriptionTransferPayload,
};
use super::store::JobStore;

mod manual_transfer;
mod metadata_scrape;
mod push_dispatch;
mod subscription_transfer;

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
        info!("后台任务 worker 已启动");
        while let Some(job_id) = self.receiver.recv().await {
            let runner = self.worker_for_job();
            let task_job_id = job_id.clone();
            let handle = tokio::spawn(async move {
                let result = runner.run_job(&task_job_id).await;
                let canceled = result.is_err() && runner.is_canceled(&task_job_id).await;
                (task_job_id, result, canceled)
            });

            match handle.await {
                Ok((task_job_id, Ok(()), _)) => {
                    let _ = task_job_id;
                }
                Ok((task_job_id, Err(_), true)) => {
                    info!("任务 {} 已取消", task_job_id);
                }
                Ok((task_job_id, Err(e), false)) => {
                    error!("任务 {} 执行失败: {}", task_job_id, e);
                }
                Err(join_error) => {
                    if self.is_canceled(&job_id).await {
                        info!("任务 {} 已取消", job_id);
                    } else {
                        error!("任务 {} panic: {}", job_id, join_error);
                        if let Err(error) = self.fail_panicked_job(&job_id, &join_error).await {
                            error!("标记 panic 任务 {} 失败: {}", job_id, error);
                        }
                    }
                }
            }
        }
        warn!("后台任务 worker 已停止");
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

    async fn fail_panicked_job(
        &self,
        job_id: &str,
        join_error: &tokio::task::JoinError,
    ) -> Result<()> {
        self.store
            .update(job_id, |job| {
                job.status = JobStatus::Failed;
                job.progress = 100;
                job.message = "后台任务异常退出，可重试".to_string();
                job.error = Some(format!("后台任务 panic: {}", join_error));
                job.finished_at = Some(now());
            })
            .await?;
        Ok(())
    }

    async fn run_job(&self, job_id: &str) -> Result<()> {
        let Some(job) = self.store.get(job_id).await else {
            warn!("任务不存在: {}", job_id);
            return Ok(());
        };

        if job.status != JobStatus::Queued {
            info!("跳过非排队任务 {}: {:?}", job_id, job.status);
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
