mod config;
mod error;
mod models;

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

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    tracing::info!("🦀 Starting my-media-sub Rust version...");

    // 加载配置
    let config = Config::load()?;
    tracing::info!("✅ Configuration loaded");

    // 构建路由
    let app = Router::new()
        // API 路由
        .route("/api/health", get(health_check))
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
