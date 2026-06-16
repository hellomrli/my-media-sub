pub mod drive;
pub mod notifications;
pub mod push;
pub mod search;
pub mod settings;
pub mod subscriptions;
pub mod transfer;

use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use crate::app::AppContext;
use crate::store::SettingsStore;

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

async fn basic_auth(
    State(settings_store): State<Arc<SettingsStore>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if req.uri().path() == "/health" {
        return next.run(req).await;
    }

    let settings = settings_store.get().await;
    if settings.app_password.is_empty() {
        return next.run(req).await;
    }

    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Basic "))
        .and_then(|encoded| general_purpose::STANDARD.decode(encoded).ok())
        .and_then(|decoded| String::from_utf8(decoded).ok())
        .and_then(|credentials| {
            let (username, password) = credentials.split_once(':')?;
            Some(username == settings.app_username && password == settings.app_password)
        })
        .unwrap_or(false);

    if authorized {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, r#"Basic realm="my-media-sub""#)],
            "Unauthorized",
        )
            .into_response()
    }
}

/// 创建主应用路由
pub fn create_app(context: Arc<AppContext>) -> Router {
    let settings_store = context.settings_store.clone();
    let auth_state = settings_store.clone();

    // CORS 配置
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 静态文件服务
    let serve_static = ServeDir::new("static").append_index_html_on_directories(true);

    // 构建路由：API 优先，静态文件作为 fallback
    Router::new()
        .route("/health", get(health))
        .merge(subscriptions::routes(
            context.subscription_store.clone(),
            settings_store.clone(),
            context.check_service.clone(),
            context.transfer_service.clone(),
        ))
        .merge(settings::routes(
            settings_store.clone(),
            context.scheduler.clone(),
        ))
        .merge(search::routes(
            context.pansou_client.clone(),
            settings_store.clone(),
        ))
        .merge(notifications::routes(context.notification_store.clone()))
        .merge(drive::routes(settings_store.clone()))
        .merge(transfer::routes(
            settings_store.clone(),
            context.notification_store.clone(),
        ))
        .merge(push::routes(settings_store))
        .fallback_service(serve_static)
        .layer(middleware::from_fn_with_state(auth_state, basic_auth))
        .layer(cors)
}
