use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::response::ApiResponse;
use crate::app::AppContext;
use crate::error::Result;

#[derive(Deserialize)]
struct StorageCompactRequest {
    #[serde(default)]
    confirmation: String,
}

#[derive(Serialize)]
struct StorageCompactResult {
    removed_subscription_history: usize,
    removed_notifications: usize,
    removed_jobs: usize,
    removed_automation_events: usize,
}

async fn compact_storage(
    State(context): State<Arc<AppContext>>,
    Json(request): Json<StorageCompactRequest>,
) -> Result<Json<ApiResponse<StorageCompactResult>>> {
    if request.confirmation != "COMPACT JSON" {
        return Err(crate::error::AppError::Validation(
            "整理确认文本必须为 COMPACT JSON".to_string(),
        ));
    }
    let removed_subscription_history = context.subscription_store.compact().await?;
    let removed_notifications = context.notification_store.compact().await?;
    let removed_jobs = context.job_store.compact().await?;
    let removed_automation_events = context.automation_event_store.compact().await?;
    context.settings_store.compact().await?;
    Ok(Json(ApiResponse::with_message(
        StorageCompactResult {
            removed_subscription_history,
            removed_notifications,
            removed_jobs,
            removed_automation_events,
        },
        "历史保留策略已重新应用，所有 Store 已改写为紧凑 JSON",
    )))
}

pub fn routes(context: Arc<AppContext>) -> Router {
    Router::new()
        .route("/api/storage/compact", post(compact_storage))
        .with_state(context)
}
