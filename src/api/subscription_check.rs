use axum::{
    extract::{Path, State},
    Json,
    response::IntoResponse,
};
use serde_json::json;

use crate::{
    error::Result,
    services::SubscriptionChecker,
    AppState,
};

/// 检查单个订阅
pub async fn check_subscription(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let checker = SubscriptionChecker::new(
        state.config.clone(),
        state.subscriptions.clone(),
        state.resources.clone(),
    );

    let new_resources = checker.check_subscription(&id).await?;

    Ok(Json(json!({
        "subscription_id": id,
        "new_resources_count": new_resources.len(),
        "resources": new_resources
    })))
}

/// 检查所有订阅
pub async fn check_all_subscriptions(
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let checker = SubscriptionChecker::new(
        state.config.clone(),
        state.subscriptions.clone(),
        state.resources.clone(),
    );

    let total_new = checker.check_all().await?;

    Ok(Json(json!({
        "total_new_resources": total_new,
        "message": format!("检查完成，发现 {} 个新资源", total_new)
    })))
}
