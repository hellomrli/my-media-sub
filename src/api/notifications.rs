use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;

use crate::error::Result;
use crate::models::Notification;
use crate::store::NotificationStore;

/// 通知路由状态
pub struct NotificationState {
    pub store: Arc<NotificationStore>,
}

/// 通用响应
#[derive(Serialize)]
struct Response<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
}

impl<T> Response<T> {
    fn ok(data: T) -> Self {
        Self { data: Some(data) }
    }
}

/// 列出通知
async fn list_notifications(
    State(state): State<Arc<NotificationState>>,
) -> Result<Json<Response<Vec<Notification>>>> {
    let notifications = state.store.list(true).await;
    Ok(Json(Response::ok(notifications)))
}

/// 标记已读
async fn mark_read(
    State(state): State<Arc<NotificationState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    state.store.mark_read(Some(&id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 全部已读
async fn mark_all_read(State(state): State<Arc<NotificationState>>) -> Result<impl IntoResponse> {
    state.store.mark_read(None).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 清空通知
async fn clear_notifications(
    State(state): State<Arc<NotificationState>>,
) -> Result<impl IntoResponse> {
    state.store.clear().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 创建通知路由
pub fn routes(store: Arc<NotificationStore>) -> Router {
    let state = Arc::new(NotificationState { store });

    Router::new()
        .route("/api/notifications", get(list_notifications))
        .route("/api/notifications/{id}/read", post(mark_read))
        .route("/api/notifications/read-all", post(mark_all_read))
        .route("/api/notifications/clear", post(clear_notifications))
        .with_state(state)
}
