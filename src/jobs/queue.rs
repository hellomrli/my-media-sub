use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::error::{AppError, Result};
use crate::services::{MetadataService, SubscriptionTransferService};
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};
use crate::utils::metrics::global_metrics;

use super::model::{
    job_idempotency_key, now, Job, JobKind, JobPriority, JobStatus, ManualTransferPayload,
    MetadataScrapePayload, PushDispatchPayload, SubscriptionTransferPayload,
};
use super::store::JobStore;
use super::worker::JobWorker;

// JobStore 最多保留 500 条记录。恢复阶段在 Worker 启动前装入全部有效 queued
// 信号，略大的容量可保证恢复不会因通道背压阻塞。
const JOB_SIGNAL_CAPACITY: usize = 512;

pub struct JobQueue {
    store: Arc<JobStore>,
    sender: mpsc::Sender<String>,
}

impl JobQueue {
    pub async fn new(
        store: Arc<JobStore>,
        settings_store: Arc<SettingsStore>,
        subscription_store: Arc<SubscriptionStore>,
        notification_store: Arc<NotificationStore>,
        metadata_service: Arc<MetadataService>,
        transfer_service: Arc<SubscriptionTransferService>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(JOB_SIGNAL_CAPACITY);
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
        // 构造完成前同步等待恢复扫描结束，避免 AppContext 已对外提供新提交后，
        // 恢复协程把本次进程刚认领的 Running 任务误判为上次重启遗留任务。
        recover_jobs(store.clone(), sender.clone()).await;
        // 恢复信号完整进入通道后再启动调度器，使其能在第一次认领前看到全部
        // 优先级，而不是按持久化遍历顺序抢跑。
        tokio::spawn(worker.run());

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
                match job.status {
                    JobStatus::Queued => {}
                    JobStatus::Running if job.kind == JobKind::MetadataScrape => {}
                    JobStatus::Running => {
                        return Err(AppError::Validation(
                            "任务已开始执行，不能可靠取消；请等待任务完成后再处理结果".to_string(),
                        ));
                    }
                    JobStatus::Succeeded | JobStatus::Failed | JobStatus::Canceled => {
                        return Err(AppError::Validation(match job.status {
                            JobStatus::Succeeded => "已成功的任务不能取消".to_string(),
                            JobStatus::Failed => "已失败的任务不能取消，可选择重试".to_string(),
                            JobStatus::Canceled => "任务已经取消".to_string(),
                            JobStatus::Queued | JobStatus::Running => unreachable!(),
                        }));
                    }
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
                self.submit_job_with_priority(job.kind, title, job.payload, job.priority)
                    .await
            }
            JobStatus::Queued | JobStatus::Running => Err(AppError::Validation(
                "任务仍在队列或执行中，不能重复提交".to_string(),
            )),
            JobStatus::Succeeded => Err(AppError::Validation(
                "已成功的任务不能直接重试，避免重复转存".to_string(),
            )),
        }
    }

    pub async fn set_priority(&self, id: &str, priority: JobPriority) -> Result<Job> {
        let updated = self
            .store
            .try_update(id, |job| {
                if job.status != JobStatus::Queued {
                    return Err(AppError::Validation(
                        "只有排队中的任务可以调整优先级".to_string(),
                    ));
                }
                job.priority = priority;
                Ok(())
            })
            .await?
            .ok_or_else(|| AppError::NotFound("任务不存在".to_string()))?;

        // 再次发送 ID 会让调度器替换其待执行快照，不会创建重复任务。
        self.sender
            .send(id.to_string())
            .await
            .map_err(|_| AppError::Internal("任务队列不可用".to_string()))?;
        Ok(updated)
    }

    pub async fn successful_push_dispatch_messages(
        &self,
        event: &str,
    ) -> HashSet<(String, String)> {
        self.store
            .list()
            .await
            .into_iter()
            .filter(|job| job.kind == JobKind::PushDispatch && job.status == JobStatus::Succeeded)
            .filter_map(|job| serde_json::from_value::<PushDispatchPayload>(job.payload).ok())
            .filter(|payload| payload.event == event)
            .map(|payload| (payload.title, payload.message))
            .collect()
    }

    async fn submit_job(
        &self,
        kind: JobKind,
        title: impl Into<String>,
        payload: serde_json::Value,
    ) -> Result<Job> {
        let priority = kind.default_priority();
        self.submit_job_with_priority(kind, title, payload, priority)
            .await
    }

    async fn submit_job_with_priority(
        &self,
        kind: JobKind,
        title: impl Into<String>,
        payload: serde_json::Value,
        priority: JobPriority,
    ) -> Result<Job> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = now();
        let idempotency_key = job_idempotency_key(&kind, &payload);
        let ambient = crate::observability::current_context();
        let payload_id = |name: &str| {
            payload
                .get(name)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        };
        let request_id = ambient.request_id;
        let correlation_id = payload_id("correlation_id").or(ambient.correlation_id);
        let subscription_id = payload_id("subscription_id").or(ambient.subscription_id);
        let job = Job {
            id: id.clone(),
            kind,
            request_id,
            correlation_id,
            subscription_id,
            priority,
            attempt: 1,
            next_attempt_at: None,
            error_class: None,
            status: JobStatus::Queued,
            progress: 0,
            title: title.into(),
            message: "等待后台任务执行".to_string(),
            idempotency_key: Some(idempotency_key),
            payload,
            result: None,
            error: None,
            created_at,
            updated_at: created_at,
            started_at: None,
            finished_at: None,
        };

        let (job, created) = self.store.add_idempotent(job).await?;
        if !created {
            info!(job_id = %job.id, correlation_id = job.correlation_id.as_deref().unwrap_or(""), "job submission deduplicated");
            return Ok(job);
        }
        info!(job_id = %job.id, correlation_id = job.correlation_id.as_deref().unwrap_or(""), subscription_id = job.subscription_id.as_deref().unwrap_or(""), job_kind = job.kind.as_str(), "job queued");
        let job_kind = job.kind.clone();
        if self.sender.send(id.clone()).await.is_err() {
            mark_queue_unavailable(&self.store, &id).await?;
            return Err(AppError::Internal("任务队列不可用".to_string()));
        }
        if matches!(
            job_kind,
            JobKind::ManualTransfer | JobKind::SubscriptionTransfer
        ) {
            global_metrics().increment_transfer_tasks();
        }

        Ok(job)
    }
}

pub(crate) async fn recover_jobs(store: Arc<JobStore>, sender: mpsc::Sender<String>) {
    let mut jobs = store.list().await;
    jobs.sort_by_key(|job| job.created_at);
    let mut queued = Vec::new();
    let mut interrupted = 0usize;
    let mut skipped_push_dispatch = 0usize;
    let mut seen_idempotency_keys = HashSet::new();
    let interrupted_idempotency_keys = jobs
        .iter()
        .filter(|job| job.status == JobStatus::Running && job.kind != JobKind::PushDispatch)
        .filter_map(|job| job.idempotency_key.clone())
        .collect::<HashSet<_>>();

    for job in jobs {
        match job.status {
            JobStatus::Queued if job.kind == JobKind::PushDispatch => {
                skipped_push_dispatch += 1;
                if let Err(e) = mark_push_dispatch_skipped_after_restart(&store, &job.id).await {
                    warn!("跳过重启前推送任务 {} 失败: {}", job.id, e);
                }
            }
            JobStatus::Queued => {
                let duplicate = job.idempotency_key.as_ref().is_some_and(|key| {
                    interrupted_idempotency_keys.contains(key)
                        || !seen_idempotency_keys.insert(key.clone())
                });
                if duplicate {
                    if let Err(error) = mark_duplicate_after_restart(&store, &job.id).await {
                        warn!("标记重启重复任务 {} 失败: {}", job.id, error);
                    }
                } else {
                    queued.push(job);
                }
            }
            JobStatus::Running if job.kind == JobKind::PushDispatch => {
                skipped_push_dispatch += 1;
                if let Err(e) = mark_push_dispatch_skipped_after_restart(&store, &job.id).await {
                    warn!("跳过重启前推送任务 {} 失败: {}", job.id, e);
                }
            }
            JobStatus::Running => {
                interrupted += 1;
                if let Err(e) = store.update(&job.id, mark_interrupted_after_restart).await {
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

    if queued_count > 0 || interrupted > 0 || skipped_push_dispatch > 0 {
        info!(
            "任务队列恢复完成: 重新入队 {} 个，标记中断 {} 个，跳过推送 {} 个",
            queued_count, interrupted, skipped_push_dispatch
        );
    }
}

fn mark_interrupted_after_restart(job: &mut Job) {
    job.status = JobStatus::Failed;
    job.progress = 100;
    job.message = "服务重启，任务已中断，可重试".to_string();
    job.error = Some("服务重启，任务已中断".to_string());
    job.finished_at = Some(now());
}

async fn mark_push_dispatch_skipped_after_restart(store: &JobStore, id: &str) -> Result<()> {
    store
        .update(id, |job| {
            job.status = JobStatus::Canceled;
            job.progress = 100;
            job.message = "服务重启，已跳过未完成推送，避免重复发送".to_string();
            job.error = None;
            job.finished_at = Some(now());
        })
        .await?;
    Ok(())
}

async fn mark_duplicate_after_restart(store: &JobStore, id: &str) -> Result<()> {
    store
        .update(id, |job| {
            job.status = JobStatus::Canceled;
            job.progress = 100;
            job.message = "服务重启，已跳过重复幂等任务".to_string();
            job.error = None;
            job.finished_at = Some(now());
        })
        .await?;
    Ok(())
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
        test_job_with_kind(id, status, JobKind::MetadataScrape)
    }

    fn test_job_with_kind(id: &str, status: JobStatus, kind: JobKind) -> Job {
        Job {
            id: id.to_string(),
            kind,
            request_id: None,
            correlation_id: None,
            subscription_id: None,
            priority: JobPriority::Normal,
            attempt: 1,
            next_attempt_at: None,
            error_class: None,
            status,
            progress: 30,
            title: "测试任务".to_string(),
            message: "running".to_string(),
            idempotency_key: None,
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

    #[tokio::test]
    async fn cancel_rejects_running_side_effect_jobs() {
        let tmp = std::env::temp_dir().join(format!(
            "my-media-sub-job-cancel-running-transfer-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(JobStore::new(&tmp));
        store.load().await.unwrap();
        store
            .add(test_job_with_kind(
                "running-transfer",
                JobStatus::Running,
                JobKind::SubscriptionTransfer,
            ))
            .await
            .unwrap();
        let (sender, _receiver) = mpsc::channel(1);
        let queue = JobQueue {
            store: store.clone(),
            sender,
        };

        let error = queue.cancel("running-transfer").await.unwrap_err();

        assert!(matches!(error, AppError::Validation(_)));
        assert_eq!(
            store.get("running-transfer").await.unwrap().status,
            JobStatus::Running
        );
        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn recover_jobs_skips_push_dispatch_after_restart() {
        let tmp = std::env::temp_dir().join(format!(
            "my-media-sub-job-recover-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(JobStore::new(&tmp));
        store.load().await.unwrap();
        let mut push_job = test_job_with_kind("push", JobStatus::Queued, JobKind::PushDispatch);
        push_job.payload = json!({
            "event": "download_completed",
            "title": "下载完成: A.mkv",
            "message": "文件：A.mkv",
            "level": "success"
        });
        store.add(push_job).await.unwrap();
        store
            .add(test_job_with_kind(
                "metadata",
                JobStatus::Queued,
                JobKind::MetadataScrape,
            ))
            .await
            .unwrap();

        let (sender, mut receiver) = mpsc::channel(4);
        recover_jobs(store.clone(), sender).await;

        let skipped = store.get("push").await.unwrap();
        assert_eq!(skipped.status, JobStatus::Canceled);
        assert_eq!(skipped.progress, 100);
        assert!(skipped.message.contains("避免重复发送"));
        assert_eq!(receiver.try_recv().unwrap(), "metadata");
        assert!(receiver.try_recv().is_err());

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn recover_jobs_keeps_oldest_queued_idempotent_task() {
        let tmp = std::env::temp_dir().join(format!(
            "my-media-sub-job-recover-idempotent-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(JobStore::new(&tmp));
        store.load().await.unwrap();

        let mut oldest = test_job("oldest", JobStatus::Queued);
        oldest.created_at = 1;
        oldest.idempotency_key = Some("same-work".to_string());
        let mut newest = test_job("newest", JobStatus::Queued);
        newest.created_at = 2;
        newest.idempotency_key = Some("same-work".to_string());
        store.add(oldest).await.unwrap();
        store.add(newest).await.unwrap();

        let (sender, mut receiver) = mpsc::channel(4);
        recover_jobs(store.clone(), sender).await;

        assert_eq!(receiver.try_recv().unwrap(), "oldest");
        assert!(receiver.try_recv().is_err());
        assert_eq!(store.get("oldest").await.unwrap().status, JobStatus::Queued);
        let duplicate = store.get("newest").await.unwrap();
        assert_eq!(duplicate.status, JobStatus::Canceled);
        assert!(duplicate.message.contains("重复幂等任务"));

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn recover_jobs_does_not_requeue_duplicate_of_interrupted_task() {
        let tmp = std::env::temp_dir().join(format!(
            "my-media-sub-job-recover-interrupted-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(JobStore::new(&tmp));
        store.load().await.unwrap();

        let mut running = test_job("running", JobStatus::Running);
        running.created_at = 1;
        running.idempotency_key = Some("same-work".to_string());
        let mut queued = test_job("queued", JobStatus::Queued);
        queued.created_at = 2;
        queued.idempotency_key = Some("same-work".to_string());
        store.add(running).await.unwrap();
        store.add(queued).await.unwrap();

        let (sender, mut receiver) = mpsc::channel(4);
        recover_jobs(store.clone(), sender).await;

        assert!(receiver.try_recv().is_err());
        assert_eq!(
            store.get("running").await.unwrap().status,
            JobStatus::Failed
        );
        assert_eq!(
            store.get("queued").await.unwrap().status,
            JobStatus::Canceled
        );

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn successful_push_dispatch_messages_lists_succeeded_payloads() {
        let tmp = std::env::temp_dir().join(format!(
            "my-media-sub-job-push-history-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(JobStore::new(&tmp));
        store.load().await.unwrap();
        let mut job = test_job_with_kind("push", JobStatus::Succeeded, JobKind::PushDispatch);
        job.payload = json!({
            "event": "download_completed",
            "title": "下载完成: A.mkv",
            "message": "文件：A.mkv",
            "level": "success"
        });
        store.add(job).await.unwrap();
        let (sender, _receiver) = mpsc::channel(1);
        let queue = JobQueue { store, sender };

        let messages = queue
            .successful_push_dispatch_messages("download_completed")
            .await;

        assert!(messages.contains(&("下载完成: A.mkv".to_string(), "文件：A.mkv".to_string())));
        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn set_priority_updates_only_queued_job_and_wakes_scheduler() {
        let tmp = std::env::temp_dir().join(format!(
            "my-media-sub-job-priority-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(JobStore::new(&tmp));
        store.load().await.unwrap();
        store
            .add(test_job("queued-priority", JobStatus::Queued))
            .await
            .unwrap();
        store
            .add(test_job("running-priority", JobStatus::Running))
            .await
            .unwrap();
        let (sender, mut receiver) = mpsc::channel(2);
        let queue = JobQueue {
            store: store.clone(),
            sender,
        };

        let updated = queue
            .set_priority("queued-priority", JobPriority::High)
            .await
            .unwrap();
        assert_eq!(updated.priority, JobPriority::High);
        assert_eq!(receiver.recv().await.as_deref(), Some("queued-priority"));

        let error = queue
            .set_priority("running-priority", JobPriority::Low)
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::Validation(_)));
        assert_eq!(
            store.get("running-priority").await.unwrap().priority,
            JobPriority::Normal
        );

        let _ = std::fs::remove_file(tmp);
    }
}
