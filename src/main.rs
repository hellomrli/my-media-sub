mod api;
mod clients;
mod config;
mod error;
mod models;
mod services;
mod store;

use axum::{
    routing::get,
    Router,
    Json,
    http::StatusCode,
};
use serde::Serialize;
use std::net::SocketAddr;
use tower_http::{
    services::ServeDir,
    trace::TraceLayer,
};
use tracing_subscriber;

use config::Config;
use error::Result;
use models::{Subscription, Resource};
use store::JsonStore;
use std::sync::Arc;

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    message: String,
}

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: "0.6.0-rust".to_string(),
        message: "Rust version running!".to_string(),
    })
}

async fn not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "404 Not Found")
}

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub subscriptions: Arc<JsonStore<Subscription>>,
    pub resources: Arc<JsonStore<Resource>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    tracing::info!("🦀 Starting my-media-sub Rust version...");

    // 加载配置
    let config = Config::load()?;
    tracing::info!("✅ Configuration loaded");

    // 初始化数据存储（使用新的 Rust 专用文件）
    let subscriptions = Arc::new(JsonStore::new(config.data_dir.join("subscriptions_rust.json")));
    let resources = Arc::new(JsonStore::new(config.data_dir.join("resources_rust.json")));
    
    subscriptions.load().await?;
    resources.load().await?;
    tracing::info!("✅ Data stores loaded");

    let state = AppState {
        config: Arc::new(config.clone()),
        subscriptions,
        resources,
    };

    // 构建路由
    let app = Router::new()
        // 健康检查
        .route("/api/health", get(health_check))
        // 订阅 API
        .route("/api/subscriptions", get(api::subscriptions::list_subscriptions))
        .route("/api/subscriptions", axum::routing::post(api::subscriptions::create_subscription))
        .route("/api/subscriptions/{id}", get(api::subscriptions::get_subscription))
        .route("/api/subscriptions/{id}", axum::routing::delete(api::subscriptions::delete_subscription))
        .route("/api/subscriptions/{id}/status", axum::routing::put(api::subscriptions::update_subscription_status))
        // 资源 API
        .route("/api/resources", get(api::resources::list_resources))
        .route("/api/resources", axum::routing::post(api::resources::add_manual_resource))
        .route("/api/resources/{id}", get(api::resources::get_resource))
        .route("/api/resources/{id}", axum::routing::delete(api::resources::delete_resource))
        .route("/api/subscriptions/{id}/resources", get(api::resources::list_subscription_resources))
        // 夸克 API
        .route("/api/quark/probe", get(api::quark::probe_share))
        .route("/api/quark/save", axum::routing::post(api::quark::save_share))
        // 搜索 API
        .route("/api/search", get(api::search::search))
        // 订阅检查 API
        .route("/api/subscriptions/{id}/check", axum::routing::post(api::subscription_check::check_subscription))
        .route("/api/subscriptions/check-all", axum::routing::post(api::subscription_check::check_all_subscriptions))
        .with_state(state)
        // 404 处理
        .fallback(not_found)
        // 静态文件服务
        .fallback_service(ServeDir::new("static"))
        // 添加日志中间件
        .layer(TraceLayer::new_for_http());

    // 绑定地址
    let addr = SocketAddr::from((
        config.server.host.parse::<std::net::IpAddr>()
            .unwrap_or_else(|_| std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0))),
        config.server.port,
    ));
    
    tracing::info!("🚀 Server starting on http://{}...", addr);

    // 启动服务器
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind address");

    tracing::info!("✅ Server listening on http://{}", addr);

    axum::serve(listener, app)
        .await
        .expect("Server error");

    Ok(())
}
