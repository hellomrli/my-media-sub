use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
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
        service.handle_update_bounded(update).await;
    });
    StatusCode::OK
}

/// Telegram update 的实际体积远小于此值（文本消息通常 < 10 KiB）。
/// 认证中间件对 webhook 前缀放行（它用随机路径 + Header secret 双重认证），
/// 因此这是唯一能在验密之前限制「未认证请求解析 JSON 体」成本的地方：
/// axum 默认体限制是 2 MiB，一个扫描器即可用它放大 CPU 与带宽。
const TELEGRAM_WEBHOOK_BODY_LIMIT: usize = 64 * 1024;

pub fn routes(service: Arc<TelegramBotService>) -> Router {
    Router::new()
        .route("/api/telegram/audits", get(audits))
        .route("/api/telegram/webhook/{path_secret}", post(webhook))
        .layer(DefaultBodyLimit::max(TELEGRAM_WEBHOOK_BODY_LIMIT))
        .with_state(service)
}
