use axum::{
    extract::State,
    http::StatusCode,
    Json,
    response::IntoResponse,
};
use serde_json::json;

use crate::{
    error::Result,
    models::{Resource, ResourceSource, ManualResourceRequest},
    AppState,
};

/// 获取所有资源
pub async fn list_resources(
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let resources = state.resources.all().await;
    Ok(Json(resources))
}

/// 获取单个资源
pub async fn get_resource(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse> {
    let resource = state.resources
        .find(|r| r.id == id)
        .await
        .ok_or_else(|| crate::error::AppError::NotFound(format!("Resource {} not found", id)))?;
    
    Ok(Json(resource))
}

/// 手动添加资源
pub async fn add_manual_resource(
    State(state): State<AppState>,
    Json(req): Json<ManualResourceRequest>,
) -> Result<impl IntoResponse> {
    let mut resource = Resource::new(
        req.title,
        req.share_url,
        req.share_pwd,
        ResourceSource::Manual,
    );
    
    resource.subscription_id = req.subscription_id;
    
    state.resources.add(resource.clone()).await?;
    
    Ok((StatusCode::CREATED, Json(resource)))
}

/// 删除资源
pub async fn delete_resource(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse> {
    let removed = state.resources
        .remove(|r| r.id == id)
        .await?;
    
    if removed {
        Ok(Json(json!({ "success": true, "message": "Resource deleted" })))
    } else {
        Err(crate::error::AppError::NotFound(format!("Resource {} not found", id)))
    }
}

/// 获取订阅的所有资源
pub async fn list_subscription_resources(
    State(state): State<AppState>,
    axum::extract::Path(subscription_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse> {
    let resources = state.resources
        .filter(|r| r.subscription_id.as_ref() == Some(&subscription_id))
        .await;
    
    Ok(Json(resources))
}
