use axum::{
    extract::{Path, State},
    Json,
    response::IntoResponse,
};
use serde_json::json;

use crate::{
    error::Result,
    services::AutoSaveService,
    AppState,
};

/// 转存单个资源
pub async fn save_resource(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let service = AutoSaveService::new(
        state.config.clone(),
        state.resources.clone(),
    );

    service.save_resource(&id).await?;

    Ok(Json(json!({
        "resource_id": id,
        "message": "转存成功"
    })))
}

/// 转存所有待处理资源
pub async fn save_all_pending(
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let service = AutoSaveService::new(
        state.config.clone(),
        state.resources.clone(),
    );

    let count = service.save_all_pending().await?;

    Ok(Json(json!({
        "saved_count": count,
        "message": format!("成功转存 {} 个资源", count)
    })))
}
