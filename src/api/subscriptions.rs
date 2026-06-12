use axum::{
    extract::State,
    http::StatusCode,
    Json,
    response::IntoResponse,
};
use serde_json::json;

use crate::{
    error::Result,
    models::{Subscription, SubscriptionStatus, MediaType, CreateSubscriptionRequest},
    AppState,
};

/// 获取所有订阅
pub async fn list_subscriptions(
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let subscriptions = state.subscriptions.all().await;
    Ok(Json(subscriptions))
}

/// 获取单个订阅
pub async fn get_subscription(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse> {
    let subscription = state.subscriptions
        .find(|s| s.id == id)
        .await
        .ok_or_else(|| crate::error::AppError::NotFound(format!("Subscription {} not found", id)))?;
    
    Ok(Json(subscription))
}

/// 创建订阅
pub async fn create_subscription(
    State(state): State<AppState>,
    Json(req): Json<CreateSubscriptionRequest>,
) -> Result<impl IntoResponse> {
    let mut subscription = Subscription::new(
        req.name,
        req.media_type,
        req.keywords,
    );
    
    subscription.share_url = req.share_url;
    subscription.share_pwd = req.share_pwd;
    subscription.save_dir = req.save_dir;
    subscription.notes = req.notes;
    
    state.subscriptions.add(subscription.clone()).await?;
    
    Ok((StatusCode::CREATED, Json(subscription)))
}

/// 删除订阅
pub async fn delete_subscription(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse> {
    let removed = state.subscriptions
        .remove(|s| s.id == id)
        .await?;
    
    if removed {
        Ok(Json(json!({ "success": true, "message": "Subscription deleted" })))
    } else {
        Err(crate::error::AppError::NotFound(format!("Subscription {} not found", id)))
    }
}

/// 更新订阅状态
pub async fn update_subscription_status(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(status): Json<SubscriptionStatus>,
) -> Result<impl IntoResponse> {
    let updated = state.subscriptions
        .update(
            |s| s.id == id,
            |s| s.status = status,
        )
        .await?;
    
    if updated {
        let subscription = state.subscriptions.find(|s| s.id == id).await.unwrap();
        Ok(Json(subscription))
    } else {
        Err(crate::error::AppError::NotFound(format!("Subscription {} not found", id)))
    }
}
