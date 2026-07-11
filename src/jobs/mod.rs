mod model;
mod queue;
pub(crate) mod reliability;
mod scheduler;
mod store;
mod worker;

pub use model::{
    Job, JobErrorClass, JobKind, JobPriority, JobStatus, ManualTransferPayload,
    MetadataScrapePayload, PushDispatchPayload, SubscriptionTransferPayload,
};
pub use queue::JobQueue;
pub use store::JobStore;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::mpsc;

    use super::model::{Job, JobKind, JobPriority, JobStatus};
    use super::queue::recover_jobs;
    use super::store::JobStore;

    #[tokio::test]
    async fn test_job_store_add_update_list() {
        let tmp =
            std::env::temp_dir().join(format!("my-media-sub-jobs-{}.json", uuid::Uuid::new_v4()));
        let store = JobStore::new(&tmp);
        store.load().await.unwrap();

        let job = Job {
            id: "job1".to_string(),
            kind: JobKind::ManualTransfer,
            priority: JobPriority::Normal,
            attempt: 1,
            next_attempt_at: None,
            error_class: None,
            status: JobStatus::Queued,
            progress: 0,
            title: "测试任务".to_string(),
            message: "queued".to_string(),
            idempotency_key: None,
            payload: json!({"url": "https://pan.quark.cn/s/test"}),
            result: None,
            error: None,
            created_at: 1,
            updated_at: 1,
            started_at: None,
            finished_at: None,
        };

        store.add(job).await.unwrap();
        store
            .update("job1", |job| {
                job.status = JobStatus::Succeeded;
                job.progress = 100;
            })
            .await
            .unwrap();

        let loaded = store.get("job1").await.unwrap();
        assert_eq!(loaded.status, JobStatus::Succeeded);
        assert_eq!(store.list().await.len(), 1);
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&tmp).unwrap()).unwrap();
        assert_eq!(persisted["schema_version"], 1);
        assert_eq!(persisted["data"].as_array().unwrap().len(), 1);

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn test_recover_jobs_requeues_queued_and_marks_running_failed() {
        let tmp =
            std::env::temp_dir().join(format!("my-media-sub-jobs-{}.json", uuid::Uuid::new_v4()));
        let store = Arc::new(JobStore::new(&tmp));
        store.load().await.unwrap();

        store
            .add(Job {
                id: "running".to_string(),
                kind: JobKind::ManualTransfer,
                priority: JobPriority::Normal,
                attempt: 1,
                next_attempt_at: None,
                error_class: None,
                status: JobStatus::Running,
                progress: 50,
                title: "运行中".to_string(),
                message: "running".to_string(),
                idempotency_key: None,
                payload: json!({"url": "https://pan.quark.cn/s/running"}),
                result: None,
                error: None,
                created_at: 1,
                updated_at: 1,
                started_at: Some(1),
                finished_at: None,
            })
            .await
            .unwrap();
        store
            .add(Job {
                id: "queued".to_string(),
                kind: JobKind::ManualTransfer,
                priority: JobPriority::Normal,
                attempt: 1,
                next_attempt_at: None,
                error_class: None,
                status: JobStatus::Queued,
                progress: 0,
                title: "排队中".to_string(),
                message: "queued".to_string(),
                idempotency_key: None,
                payload: json!({"url": "https://pan.quark.cn/s/queued"}),
                result: None,
                error: None,
                created_at: 2,
                updated_at: 2,
                started_at: None,
                finished_at: None,
            })
            .await
            .unwrap();

        let (sender, mut receiver) = mpsc::channel(10);
        recover_jobs(store.clone(), sender).await;

        assert_eq!(receiver.recv().await.as_deref(), Some("queued"));
        let running = store.get("running").await.unwrap();
        assert_eq!(running.status, JobStatus::Failed);
        assert!(running.message.contains("可重试"));

        let _ = std::fs::remove_file(tmp);
    }
}
