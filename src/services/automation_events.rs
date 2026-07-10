use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::jobs::{Job, JobKind, JobStatus, JobStore};
use crate::models::{AutomationEvent, AutomationStage, AutomationStatus};
use crate::store::AutomationEventStore;

pub fn start_job_event_projection(
    job_store: Arc<JobStore>,
    event_store: Arc<AutomationEventStore>,
) {
    let mut receiver = job_store.subscribe();
    tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(job) => {
                    if let Err(error) = project_job(&event_store, &job).await {
                        tracing::warn!("记录 Job 自动化事件失败: {}", error);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!("Job 自动化事件投影滞后，跳过 {} 条中间状态", skipped);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

pub async fn project_job(
    event_store: &AutomationEventStore,
    job: &Job,
) -> crate::error::Result<()> {
    let stage = job_stage(&job.kind);
    let status = job_status(&job.status);
    let correlation_id = job
        .payload
        .get("correlation_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(&job.id)
        .to_string();
    let mut event = AutomationEvent::new(
        format!("job:{}", job.id),
        correlation_id,
        stage,
        status,
        job.created_at,
    );
    event.subscription_id = job
        .payload
        .get("subscription_id")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    event.episode = job
        .payload
        .get("episode")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok());
    event.job_id = Some(job.id.clone());
    event.message = job.message.clone();
    event.error = job.error.clone().unwrap_or_default();
    event.created_at = job.created_at;
    event.updated_at = job.updated_at;
    event.started_at = job.started_at;
    event.finished_at = job.finished_at;
    event.metadata = BTreeMap::from([
        ("job_kind".to_string(), json!(job_kind_name(&job.kind))),
        ("progress".to_string(), json!(job.progress)),
        ("title".to_string(), json!(job.title)),
    ]);
    event_store.upsert(event).await?;
    if job.kind == JobKind::SubscriptionTransfer && job.status == JobStatus::Succeeded {
        project_transfer_stages(event_store, job).await?;
    }
    Ok(())
}

async fn project_transfer_stages(
    event_store: &AutomationEventStore,
    job: &Job,
) -> crate::error::Result<()> {
    let result = job.result.as_ref().cloned().unwrap_or_else(|| json!({}));
    let correlation_id = job
        .payload
        .get("correlation_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(&job.id);
    let subscription_id = job
        .payload
        .get("subscription_id")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let episodes = job
        .payload
        .get("file_names")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|name| crate::services::detect_episode(name).episode)
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    let episodes = if episodes.is_empty() {
        vec![None]
    } else {
        episodes.into_iter().map(Some).collect::<Vec<_>>()
    };
    let stages = [
        (
            AutomationStage::Rename,
            stage_status(
                result
                    .get("renamed_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                None,
            ),
            "重命名阶段",
            result
                .get("renamed_count")
                .cloned()
                .unwrap_or(Value::from(0)),
        ),
        (
            AutomationStage::Strm,
            stage_status(
                result
                    .get("strm_generated_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                result.get("strm_error").and_then(Value::as_str),
            ),
            "STRM 阶段",
            result
                .get("strm_generated_count")
                .cloned()
                .unwrap_or(Value::from(0)),
        ),
        (
            AutomationStage::Aria2,
            stage_status(
                result
                    .get("aria2_submitted_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                result.get("aria2_error").and_then(Value::as_str),
            ),
            "Aria2 阶段",
            result
                .get("aria2_submitted_count")
                .cloned()
                .unwrap_or(Value::from(0)),
        ),
        (
            AutomationStage::Notification,
            if result
                .get("notification_id")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty())
            {
                AutomationStatus::Succeeded
            } else {
                AutomationStatus::Skipped
            },
            "通知阶段",
            Value::from(
                result
                    .get("notification_id")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.is_empty()) as u8,
            ),
        ),
    ];
    for episode in episodes {
        for (stage, status, message, count) in &stages {
            let mut event = AutomationEvent::new(
                format!(
                    "job:{}:{}:{}",
                    job.id,
                    stage.as_str(),
                    episode
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "all".to_string())
                ),
                correlation_id,
                *stage,
                *status,
                job.updated_at,
            );
            event.subscription_id = subscription_id.clone();
            event.episode = episode;
            event.job_id = Some(job.id.clone());
            event.message = (*message).to_string();
            event.error = match stage {
                AutomationStage::Strm => result
                    .get("strm_error")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                AutomationStage::Aria2 => result
                    .get("aria2_error")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                _ => String::new(),
            };
            event.metadata.insert("count".to_string(), count.clone());
            event_store.add(event).await?;
        }
    }
    Ok(())
}

fn stage_status(count: u64, error: Option<&str>) -> AutomationStatus {
    if error.is_some_and(|value| !value.trim().is_empty()) {
        AutomationStatus::Failed
    } else if count > 0 {
        AutomationStatus::Succeeded
    } else {
        AutomationStatus::Skipped
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn record_stage_event(
    event_store: Option<&Arc<AutomationEventStore>>,
    correlation_id: &str,
    subscription_id: Option<&str>,
    episode: Option<i32>,
    job_id: Option<&str>,
    stage: AutomationStage,
    status: AutomationStatus,
    message: impl Into<String>,
    error: impl Into<String>,
    metadata: BTreeMap<String, Value>,
) {
    let Some(store) = event_store else {
        return;
    };
    let now = crate::utils::unix_now();
    let event_id = format!(
        "{}:{}:{}",
        correlation_id,
        stage.as_str(),
        episode
            .map(|value| value.to_string())
            .unwrap_or_else(|| "all".to_string())
    );
    let mut event = AutomationEvent::new(event_id, correlation_id, stage, status, now);
    event.subscription_id = subscription_id.map(ToString::to_string);
    event.episode = episode;
    event.job_id = job_id.map(ToString::to_string);
    event.message = message.into();
    event.error = error.into();
    event.metadata = metadata;
    if let Err(error) = store.upsert(event).await {
        tracing::warn!("记录自动化阶段事件失败: {}", error);
    }
}

fn job_stage(kind: &JobKind) -> AutomationStage {
    match kind {
        JobKind::ManualTransfer | JobKind::SubscriptionTransfer => AutomationStage::CloudTransfer,
        JobKind::MetadataScrape => AutomationStage::VersionSelect,
        JobKind::PushDispatch => AutomationStage::Notification,
    }
}

fn job_status(status: &JobStatus) -> AutomationStatus {
    match status {
        JobStatus::Queued => AutomationStatus::Pending,
        JobStatus::Running => AutomationStatus::Running,
        JobStatus::Succeeded => AutomationStatus::Succeeded,
        JobStatus::Failed => AutomationStatus::Failed,
        JobStatus::Canceled => AutomationStatus::Canceled,
    }
}

fn job_kind_name(kind: &JobKind) -> &'static str {
    match kind {
        JobKind::ManualTransfer => "manual_transfer",
        JobKind::SubscriptionTransfer => "subscription_transfer",
        JobKind::MetadataScrape => "metadata_scrape",
        JobKind::PushDispatch => "push_dispatch",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn job_projection_creates_structured_event_without_notification_text_parsing() {
        let path = std::env::temp_dir().join(format!("events-{}.json", uuid::Uuid::new_v4()));
        let store = AutomationEventStore::new(&path);
        let now = crate::utils::unix_now();
        let job = Job {
            id: "job-1".to_string(),
            kind: JobKind::SubscriptionTransfer,
            status: JobStatus::Succeeded,
            progress: 100,
            title: "transfer".to_string(),
            message: "done".to_string(),
            idempotency_key: None,
            payload: json!({"subscription_id":"sub-1","correlation_id":"check-1"}),
            result: None,
            error: None,
            created_at: now,
            updated_at: now + 1,
            started_at: Some(now),
            finished_at: Some(now + 1),
        };
        project_job(&store, &job).await.unwrap();
        let events = store.list_by_subscription("sub-1", 10).await;
        let cloud = events
            .iter()
            .find(|event| event.stage == AutomationStage::CloudTransfer)
            .unwrap();
        assert_eq!(cloud.status, AutomationStatus::Succeeded);
        assert_eq!(cloud.correlation_id, "check-1");
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn job_projection_updates_one_current_lifecycle() {
        let path = std::env::temp_dir().join(format!("events-{}.json", uuid::Uuid::new_v4()));
        let store = AutomationEventStore::new(&path);
        let now = crate::utils::unix_now();
        let mut job = Job {
            id: "job-lifecycle".to_string(),
            kind: JobKind::MetadataScrape,
            status: JobStatus::Queued,
            progress: 0,
            title: "metadata".to_string(),
            message: "queued".to_string(),
            idempotency_key: None,
            payload: json!({"subscription_id":"sub-1"}),
            result: None,
            error: None,
            created_at: now,
            updated_at: now,
            started_at: None,
            finished_at: None,
        };
        project_job(&store, &job).await.unwrap();
        job.status = JobStatus::Running;
        job.progress = 50;
        job.updated_at = now + 2;
        job.started_at = Some(now + 1);
        project_job(&store, &job).await.unwrap();
        job.status = JobStatus::Succeeded;
        job.progress = 100;
        job.updated_at = now + 4;
        job.finished_at = Some(now + 4);
        project_job(&store, &job).await.unwrap();

        let events = store.list_by_job("job-lifecycle", 10).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].status, AutomationStatus::Succeeded);
        assert_eq!(events[0].metadata["progress"], json!(100));
        assert_eq!(events[0].duration_seconds(), Some(3));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn completed_transfer_projects_episode_stage_outcomes() {
        let path = std::env::temp_dir().join(format!("events-{}.json", uuid::Uuid::new_v4()));
        let store = AutomationEventStore::new(&path);
        let job = Job {
            id: "job-transfer".to_string(),
            kind: JobKind::SubscriptionTransfer,
            status: JobStatus::Succeeded,
            progress: 100,
            title: "transfer".to_string(),
            message: "done".to_string(),
            idempotency_key: None,
            payload: json!({
                "subscription_id":"sub-1",
                "correlation_id":"check-1",
                "file_names":["Show.S01E04.mkv"]
            }),
            result: Some(json!({
                "renamed_count":1,
                "strm_generated_count":1,
                "strm_error":null,
                "aria2_submitted_count":0,
                "aria2_error":"aria2 unavailable",
                "notification_id":"notification-1"
            })),
            error: None,
            created_at: crate::utils::unix_now(),
            updated_at: crate::utils::unix_now(),
            started_at: Some(crate::utils::unix_now()),
            finished_at: Some(crate::utils::unix_now()),
        };
        project_job(&store, &job).await.unwrap();
        let events = store.list_by_job("job-transfer", 20).await;
        assert!(events
            .iter()
            .any(|event| event.stage == AutomationStage::CloudTransfer));
        assert!(events.iter().any(|event| {
            event.stage == AutomationStage::Rename
                && event.status == AutomationStatus::Succeeded
                && event.episode == Some(4)
        }));
        assert!(events.iter().any(|event| {
            event.stage == AutomationStage::Strm
                && event.status == AutomationStatus::Succeeded
                && event.episode == Some(4)
        }));
        assert!(events.iter().any(|event| {
            event.stage == AutomationStage::Aria2
                && event.status == AutomationStatus::Failed
                && event.episode == Some(4)
        }));
        assert!(events.iter().any(|event| {
            event.stage == AutomationStage::Notification
                && event.status == AutomationStatus::Succeeded
                && event.episode == Some(4)
        }));
        let _ = std::fs::remove_file(path);
    }
}
