use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::error::{AppError, Result};
use crate::services::{MetadataService, SubscriptionTransferService};
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};

use super::model::{
    now, Job, JobKind, JobStatus, ManualTransferPayload, MetadataScrapePayload,
    PushDispatchPayload, SubscriptionTransferPayload,
};
use super::store::JobStore;
use super::worker::JobWorker;

pub struct JobQueue {
    store: Arc<JobStore>,
    sender: mpsc::Sender<String>,
}

impl JobQueue {
    pub fn new(
        store: Arc<JobStore>,
        settings_store: Arc<SettingsStore>,
        subscription_store: Arc<SubscriptionStore>,
        notification_store: Arc<NotificationStore>,
        metadata_service: Arc<MetadataService>,
        transfer_service: Arc<SubscriptionTransferService>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(100);
        let worker = JobWorker {
            store: store.clone(),
            sender: sender.clone(),
            settings_store,
            subscription_store,
            notification_store,
            metadata_service,
            transfer_service,
            receiver,
        };
        tokio::spawn(worker.run());

        tokio::spawn(recover_jobs(store.clone(), sender.clone()));

        Self { store, sender }
    }

    pub async fn submit_manual_transfer(&self, payload: ManualTransferPayload) -> Result<Job> {
        self.submit_job(
            JobKind::ManualTransfer,
            "手动转存",
            serde_json::to_value(payload)?,
        )
        .await
    }

    pub async fn submit_subscription_transfer(
        &self,
        payload: SubscriptionTransferPayload,
    ) -> Result<Job> {
        self.submit_job(
            JobKind::SubscriptionTransfer,
            "订阅自动转存",
            serde_json::to_value(payload)?,
        )
        .await
    }

    pub async fn submit_metadata_scrape(&self, payload: MetadataScrapePayload) -> Result<Job> {
        let title = if payload.subscription_id.is_some() {
            "订阅元数据刮削"
        } else {
            "批量订阅元数据刮削"
        };
        self.submit_job(
            JobKind::MetadataScrape,
            title,
            serde_json::to_value(payload)?,
        )
        .await
    }

    pub async fn submit_push_dispatch(&self, payload: PushDispatchPayload) -> Result<Job> {
        self.submit_job(
            JobKind::PushDispatch,
            "推送派发",
            serde_json::to_value(payload)?,
        )
        .await
    }

    pub async fn cancel(&self, id: &str) -> Result<Job> {
        self.store
            .try_update(id, |job| {
                if !matches!(job.status, JobStatus::Queued | JobStatus::Running) {
                    return Err(AppError::Validation(match job.status {
                        JobStatus::Succeeded => "已成功的任务不能取消".to_string(),
                        JobStatus::Failed => "已失败的任务不能取消，可选择重试".to_string(),
                        JobStatus::Canceled => "任务已经取消".to_string(),
                        JobStatus::Queued | JobStatus::Running => unreachable!(),
                    }));
                }

                job.status = JobStatus::Canceled;
                job.progress = 100;
                job.message = "任务已取消".to_string();
                job.error = None;
                job.finished_at = Some(now());
                Ok(())
            })
            .await?
            .ok_or_else(|| AppError::NotFound("任务不存在".to_string()))
    }

    pub async fn retry(&self, id: &str) -> Result<Job> {
        let job = self
            .store
            .get(id)
            .await
            .ok_or_else(|| AppError::NotFound("任务不存在".to_string()))?;

        match job.status {
            JobStatus::Failed | JobStatus::Canceled => {
                let title = match &job.kind {
                    JobKind::ManualTransfer => "手动转存",
                    JobKind::SubscriptionTransfer => "订阅自动转存",
                    JobKind::MetadataScrape => "订阅元数据刮削",
                    JobKind::PushDispatch => "推送派发",
                };
                self.submit_job(job.kind.clone(), title, job.payload).await
            }
            JobStatus::Queued | JobStatus::Running => Err(AppError::Validation(
                "任务仍在队列或执行中，不能重复提交".to_string(),
            )),
            JobStatus::Succeeded => Err(AppError::Validation(
                "已成功的任务不能直接重试，避免重复转存".to_string(),
            )),
        }
    }

    async fn submit_job(
        &self,
        kind: JobKind,
        title: impl Into<String>,
        payload: serde_json::Value,
    ) -> Result<Job> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = now();
        let job = Job {
            id: id.clone(),
            kind,
            status: JobStatus::Queued,
            progress: 0,
            title: title.into(),
            message: "等待后台任务执行".to_string(),
            payload,
            result: None,
            error: None,
            created_at,
            updated_at: created_at,
            started_at: None,
            finished_at: None,
        };

        let job = self.store.add(job).await?;
        if self.sender.send(id.clone()).await.is_err() {
            mark_queue_unavailable(&self.store, &id).await?;
            return Err(AppError::Internal("任务队列不可用".to_string()));
        }

        Ok(job)
    }
}

pub(crate) async fn recover_jobs(store: Arc<JobStore>, sender: mpsc::Sender<String>) {
    let jobs = store.list().await;
    let mut queued = Vec::new();
    let mut interrupted = 0usize;

    for job in jobs {
        match job.status {
            JobStatus::Queued => queued.push(job),
            JobStatus::Running => {
                interrupted += 1;
                if let Err(e) = store
                    .update(&job.id, |job| {
                        job.status = JobStatus::Failed;
                        job.progress = 100;
                        job.message = "服务重启，任务已中断，可重试".to_string();
                        job.error = Some("服务重启，任务已中断".to_string());
                        job.finished_at = Some(now());
                    })
                    .await
                {
                    warn!("恢复运行中任务 {} 失败: {}", job.id, e);
                }
            }
            JobStatus::Succeeded | JobStatus::Failed | JobStatus::Canceled => {}
        }
    }

    queued.sort_by_key(|job| job.created_at);
    let queued_count = queued.len();
    for job in queued {
        if sender.send(job.id.clone()).await.is_err() {
            if let Err(e) = mark_queue_unavailable(&store, &job.id).await {
                warn!("标记恢复任务 {} 失败: {}", job.id, e);
            }
        }
    }

    if queued_count > 0 || interrupted > 0 {
        info!(
            "任务队列恢复完成: 重新入队 {} 个，标记中断 {} 个",
            queued_count, interrupted
        );
    }
}

async fn mark_queue_unavailable(store: &JobStore, id: &str) -> Result<()> {
    store
        .update(id, |job| {
            job.status = JobStatus::Failed;
            job.progress = 100;
            job.message = "任务队列不可用".to_string();
            job.error = Some("任务队列不可用".to_string());
            job.finished_at = Some(now());
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::mpsc;

    use super::*;

    fn test_job(id: &str, status: JobStatus) -> Job {
        Job {
            id: id.to_string(),
            kind: JobKind::MetadataScrape,
            status,
            progress: 30,
            title: "测试任务".to_string(),
            message: "running".to_string(),
            payload: json!({"overwrite": false}),
            result: None,
            error: None,
            created_at: 1,
            updated_at: 1,
            started_at: Some(1),
            finished_at: None,
        }
    }

    #[tokio::test]
    async fn cancel_allows_running_jobs() {
        let tmp = std::env::temp_dir().join(format!(
            "my-media-sub-job-cancel-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(JobStore::new(&tmp));
        store.load().await.unwrap();
        store
            .add(test_job("running", JobStatus::Running))
            .await
            .unwrap();
        let (sender, _receiver) = mpsc::channel(1);
        let queue = JobQueue { store, sender };

        let canceled = queue.cancel("running").await.unwrap();

        assert_eq!(canceled.status, JobStatus::Canceled);
        assert_eq!(canceled.progress, 100);
        assert!(canceled.finished_at.is_some());
        let _ = std::fs::remove_file(tmp);
    }
}
