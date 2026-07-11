use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Router,
};

use crate::services::telegram_bot::{TelegramBotService, TelegramUpdate};

#[derive(Default, serde::Deserialize)]
struct AuditQuery {
    limit: Option<usize>,
}

async fn audits(
    State(service): State<Arc<TelegramBotService>>,
    Query(query): Query<AuditQuery>,
) -> axum::Json<crate::api::response::ApiResponse<Vec<crate::store::TelegramCommandAudit>>> {
    axum::Json(crate::api::response::ApiResponse::ok(
        service.audits(query.limit.unwrap_or(100).min(500)).await,
    ))
}

async fn webhook(
    State(service): State<Arc<TelegramBotService>>,
    Path(path_secret): Path<String>,
    headers: HeaderMap,
    axum::Json(update): axum::Json<TelegramUpdate>,
) -> StatusCode {
    let header_secret = headers
        .get("x-telegram-bot-api-secret-token")
        .and_then(|value| value.to_str().ok());
    if !service.webhook_matches(&path_secret, header_secret).await {
        return StatusCode::NOT_FOUND;
    }

    tokio::spawn(async move {
        service.handle_update(update).await;
    });
    StatusCode::OK
}

pub fn routes(service: Arc<TelegramBotService>) -> Router {
    Router::new()
        .route("/api/telegram/audits", get(audits))
        .route("/api/telegram/webhook/{path_secret}", post(webhook))
        .with_state(service)
}
