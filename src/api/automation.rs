use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use super::response::ApiResponse as Response;
use crate::app::AppContext;
use crate::error::{AppError, Result};
use crate::models::{AutomationEvent, AutomationStage, AutomationStatus};

#[derive(Debug, Default, Deserialize)]
struct EventQuery {
    subscription_id: Option<String>,
    correlation_id: Option<String>,
    job_id: Option<String>,
    episode: Option<i32>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct PipelineResponse {
    events: Vec<AutomationEvent>,
    latest_by_stage: BTreeMap<String, AutomationEvent>,
    episodes: BTreeMap<i32, Vec<AutomationEvent>>,
}

#[derive(Debug, Serialize)]
struct AutomationSummary {
    total: usize,
    by_status: HashMap<String, usize>,
    by_stage: BTreeMap<String, usize>,
    recent_failed: Vec<AutomationEvent>,
    stuck: Vec<AutomationEvent>,
    retry_hotspots: BTreeMap<String, usize>,
    success_rate: f64,
}

#[derive(Debug, Serialize)]
struct RetryStageResponse {
    success: bool,
    message: String,
    event: AutomationEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_job_id: Option<String>,
}

async fn list_events(
    State(ctx): State<Arc<AppContext>>,
    Query(query): Query<EventQuery>,
) -> Result<Json<Response<Vec<AutomationEvent>>>> {
    let limit = query.limit.unwrap_or(200).clamp(1, 1_000);
    let mut events = if let Some(id) = query.subscription_id.as_deref() {
        ctx.automation_event_store
            .list_by_subscription(id, limit)
            .await
    } else if let Some(id) = query.correlation_id.as_deref() {
        ctx.automation_event_store
            .list_by_correlation(id, limit)
            .await
    } else if let Some(id) = query.job_id.as_deref() {
        ctx.automation_event_store.list_by_job(id, limit).await
    } else {
        ctx.automation_event_store.list(limit).await
    };
    if let Some(episode) = query.episode {
        events.retain(|event| event.episode == Some(episode));
    }
    Ok(Json(Response::ok(events)))
}

async fn subscription_pipeline(
    State(ctx): State<Arc<AppContext>>,
    Path(id): Path<String>,
    Query(query): Query<EventQuery>,
) -> Result<Json<Response<PipelineResponse>>> {
    if ctx.subscription_store.get(&id).await.is_none() {
        return Err(AppError::NotFound("订阅不存在".to_string()));
    }
    let mut events = ctx
        .automation_event_store
        .list_by_subscription(&id, 1_000)
        .await;
    if let Some(episode) = query.episode {
        events.retain(|event| event.episode.is_none() || event.episode == Some(episode));
    }
    Ok(Json(Response::ok(build_pipeline(events))))
}

async fn job_pipeline(
    State(ctx): State<Arc<AppContext>>,
    Path(id): Path<String>,
) -> Result<Json<Response<PipelineResponse>>> {
    if ctx.job_store.get(&id).await.is_none() {
        return Err(AppError::NotFound("任务不存在".to_string()));
    }
    let events = ctx.automation_event_store.list_by_job(&id, 1_000).await;
    Ok(Json(Response::ok(build_pipeline(events))))
}

async fn automation_summary(
    State(ctx): State<Arc<AppContext>>,
) -> Result<Json<Response<AutomationSummary>>> {
    let events = ctx.automation_event_store.list(1_000).await;
    let current_events = current_stage_events(&events);
    let now = crate::utils::unix_now();
    let mut by_status = HashMap::new();
    let mut by_stage = BTreeMap::new();
    let mut retry_hotspots = BTreeMap::new();
    let mut succeeded = 0usize;
    let mut finished = 0usize;
    for event in &current_events {
        *by_status
            .entry(event.status.as_str().to_string())
            .or_insert(0) += 1;
        *by_stage
            .entry(event.stage.as_str().to_string())
            .or_insert(0) += 1;
        if event.status.is_terminal() {
            finished += 1;
            if event.status == AutomationStatus::Succeeded {
                succeeded += 1;
            }
        }
    }
    for event in &events {
        if event.attempt > 1 || event.status == AutomationStatus::Retrying {
            *retry_hotspots
                .entry(event.stage.as_str().to_string())
                .or_insert(0) += 1;
        }
    }
    let recent_failed = current_events
        .iter()
        .filter(|event| event.status == AutomationStatus::Failed)
        .take(20)
        .cloned()
        .collect();
    let stuck = current_events
        .iter()
        .filter(|event| {
            event.status == AutomationStatus::Running
                && now.saturating_sub(event.updated_at) > 30 * 60
        })
        .take(20)
        .cloned()
        .collect();
    Ok(Json(Response::ok(AutomationSummary {
        total: current_events.len(),
        by_status,
        by_stage,
        recent_failed,
        stuck,
        retry_hotspots,
        success_rate: if finished == 0 {
            0.0
        } else {
            succeeded as f64 / finished as f64 * 100.0
        },
    })))
}

fn current_stage_events(events: &[AutomationEvent]) -> Vec<AutomationEvent> {
    let projected_job_ids = events
        .iter()
        .filter(|event| event.id.starts_with("job:") && !event.id.contains(":retry:"))
        .filter_map(|event| event.job_id.clone())
        .collect::<HashSet<_>>();
    let mut current = BTreeMap::new();
    for event in events {
        let key = (
            event.correlation_id.clone(),
            event.subscription_id.clone(),
            event.episode,
            event.stage,
        );
        current
            .entry(key)
            .and_modify(|existing: &mut AutomationEvent| {
                if (event.updated_at, event.created_at) > (existing.updated_at, existing.created_at)
                {
                    *existing = event.clone();
                }
            })
            .or_insert_with(|| event.clone());
    }
    let mut events = current.into_values().collect::<Vec<_>>();
    events.retain(|event| {
        if event.status != AutomationStatus::Retrying {
            return true;
        }
        event
            .metadata
            .get("retry_job_id")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|job_id| !projected_job_ids.contains(job_id))
    });
    events.sort_by(|left, right| {
        (right.updated_at, right.created_at).cmp(&(left.updated_at, left.created_at))
    });
    events
}

async fn retry_stage(
    State(ctx): State<Arc<AppContext>>,
    Path(id): Path<String>,
) -> Result<Json<Response<RetryStageResponse>>> {
    let original = ctx
        .automation_event_store
        .get(&id)
        .await
        .ok_or_else(|| AppError::NotFound("自动化事件不存在".to_string()))?;
    if !matches!(
        original.status,
        AutomationStatus::Failed | AutomationStatus::Canceled
    ) {
        return Err(AppError::Validation(
            "只有失败或已取消阶段可以安全重试".to_string(),
        ));
    }

    let now = crate::utils::unix_now();
    let mut retry_event = AutomationEvent::new(
        format!("{}:retry:{}", original.id, original.attempt + 1),
        original.correlation_id.clone(),
        original.stage,
        AutomationStatus::Retrying,
        now,
    );
    retry_event.subscription_id = original.subscription_id.clone();
    retry_event.episode = original.episode;
    retry_event.job_id = original.job_id.clone();
    retry_event.attempt = original.attempt + 1;
    retry_event.message = "正在安全重试阶段".to_string();
    retry_event.metadata = original.metadata.clone();
    ctx.automation_event_store.add(retry_event.clone()).await?;

    let mut new_job_id = None;
    let message = if let Some(job_id) = original.job_id.as_deref() {
        match ctx.job_queue.retry(job_id).await {
            Ok(job) => {
                new_job_id = Some(job.id.clone());
                retry_event.message = format!("已创建重试任务 {}", job.id);
                retry_event
                    .metadata
                    .insert("retry_job_id".to_string(), serde_json::json!(job.id));
                ctx.automation_event_store
                    .upsert(retry_event.clone())
                    .await?;
                retry_event.message.clone()
            }
            Err(error) => {
                retry_event.status = AutomationStatus::Failed;
                retry_event.updated_at = crate::utils::unix_now();
                retry_event.finished_at = Some(retry_event.updated_at);
                retry_event.error = error.to_string();
                ctx.automation_event_store.upsert(retry_event).await?;
                return Err(error);
            }
        }
    } else if matches!(
        original.stage,
        AutomationStage::SourceCheck | AutomationStage::FileFilter
    ) {
        let subscription_id = original
            .subscription_id
            .as_deref()
            .ok_or_else(|| AppError::Validation("事件缺少订阅 ID".to_string()))?;
        retry_event.status = AutomationStatus::Running;
        retry_event.updated_at = crate::utils::unix_now();
        retry_event.started_at = Some(retry_event.updated_at);
        retry_event.message = "正在重新检查订阅".to_string();
        ctx.automation_event_store
            .upsert(retry_event.clone())
            .await?;

        let settings = ctx.settings_store.get().await;
        match ctx
            .check_service
            .check_subscription(subscription_id, &settings.quark_cookie)
            .await
        {
            Ok(result) => {
                retry_event.status = AutomationStatus::Succeeded;
                retry_event.updated_at = crate::utils::unix_now();
                retry_event.finished_at = Some(retry_event.updated_at);
                retry_event.message = result.summary;
                retry_event.error.clear();
                ctx.automation_event_store
                    .upsert(retry_event.clone())
                    .await?;
                retry_event.message.clone()
            }
            Err(error) => {
                retry_event.status = AutomationStatus::Failed;
                retry_event.updated_at = crate::utils::unix_now();
                retry_event.finished_at = Some(retry_event.updated_at);
                retry_event.error = error.to_string();
                ctx.automation_event_store
                    .upsert(retry_event.clone())
                    .await?;
                return Err(error);
            }
        }
    } else {
        retry_event.status = AutomationStatus::Failed;
        retry_event.updated_at = crate::utils::unix_now();
        retry_event.finished_at = Some(retry_event.updated_at);
        retry_event.error = "该阶段没有可安全重试的独立处理器".to_string();
        ctx.automation_event_store.upsert(retry_event).await?;
        return Err(AppError::Validation(
            "该阶段没有可安全重试的独立处理器".to_string(),
        ));
    };

    Ok(Json(Response::ok(RetryStageResponse {
        success: true,
        message,
        event: retry_event,
        new_job_id,
    })))
}

fn build_pipeline(mut events: Vec<AutomationEvent>) -> PipelineResponse {
    events.sort_by_key(|event| (event.created_at, event.updated_at));
    let mut latest_by_stage = BTreeMap::new();
    let mut episodes = BTreeMap::<i32, Vec<AutomationEvent>>::new();
    for event in &events {
        latest_by_stage.insert(event.stage.as_str().to_string(), event.clone());
        if let Some(episode) = event.episode {
            episodes.entry(episode).or_default().push(event.clone());
        }
    }
    PipelineResponse {
        events,
        latest_by_stage,
        episodes,
    }
}

pub fn routes(ctx: Arc<AppContext>) -> Router {
    Router::new()
        .route("/api/automation/events", get(list_events))
        .route("/api/automation/summary", get(automation_summary))
        .route("/api/automation/events/{id}/retry", post(retry_stage))
        .route(
            "/api/subscriptions/{id}/pipeline",
            get(subscription_pipeline),
        )
        .route("/api/jobs/{id}/pipeline", get(job_pipeline))
        .with_state(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_collapses_historical_states_to_latest_stage_outcome() {
        let mut running = AutomationEvent::new(
            "legacy:running",
            "correlation",
            AutomationStage::CloudTransfer,
            AutomationStatus::Running,
            10,
        );
        running.updated_at = 20;
        let mut succeeded = AutomationEvent::new(
            "legacy:succeeded",
            "correlation",
            AutomationStage::CloudTransfer,
            AutomationStatus::Succeeded,
            10,
        );
        succeeded.updated_at = 30;
        let current = current_stage_events(&[running, succeeded.clone()]);
        assert_eq!(current, vec![succeeded]);

        let mut retrying = AutomationEvent::new(
            "failed:retry:2",
            "old-job",
            AutomationStage::VersionSelect,
            AutomationStatus::Retrying,
            40,
        );
        retrying
            .metadata
            .insert("retry_job_id".to_string(), serde_json::json!("new-job"));
        let mut child = AutomationEvent::new(
            "job:new-job",
            "new-job",
            AutomationStage::VersionSelect,
            AutomationStatus::Pending,
            41,
        );
        child.job_id = Some("new-job".to_string());
        assert_eq!(
            current_stage_events(&[retrying, child.clone()]),
            vec![child]
        );
    }
}
