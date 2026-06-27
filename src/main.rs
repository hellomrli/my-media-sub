mod api;
mod app;
mod clients;
mod config;
mod error;
mod jobs;
mod models;
mod services;
mod store;
mod utils;

use app::AppContext;
use error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    tracing::info!("🦀 Starting my-media-sub Rust v2...");

    // 加载配置
    let config = config::Config::load()?;
    tracing::info!("✅ Configuration loaded");
    tracing::info!("   Server: {}:{}", config.server.host, config.server.port);
    tracing::info!("   Data dir: {}", config.data_dir.display());

    let context = AppContext::new(&config).await?;
    context.start_background_services().await?;

    // 创建应用
    let app = api::create_app(context);

    // 绑定地址
    let addr = std::net::SocketAddr::from((
        config
            .server
            .host
            .parse::<std::net::IpAddr>()
            .unwrap_or_else(|_| std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0))),
        config.server.port,
    ));

    tracing::info!("🚀 Server starting on http://{}", addr);

    // 启动服务器
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("✅ Server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
