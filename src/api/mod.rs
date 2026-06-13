pub mod subscriptions;
pub mod settings;
pub mod search;
pub mod notifications;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde::Serialize;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use crate::clients::PanSouClient;
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};

/// 健康检查响应
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

/// 健康检查
async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// 创建主应用路由
pub fn create_app(
    subscription_store: Arc<SubscriptionStore>,
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    pansou_client: Arc<PanSouClient>,
) -> Router {
    // CORS 配置
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 静态文件服务
    let serve_static = ServeDir::new("static")
        .append_index_html_on_directories(true);

    // 构建路由：API 优先，静态文件作为 fallback
    Router::new()
        .route("/health", get(health))
        .merge(subscriptions::routes(subscription_store))
        .merge(settings::routes(settings_store))
        .merge(search::routes(pansou_client))
        .merge(notifications::routes(notification_store))
        .fallback_service(serve_static)  // 关键修复：使用 fallback_service
        .layer(cors)
}
