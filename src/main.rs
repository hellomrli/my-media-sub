use std::sync::Arc;
use tracing_subscriber;

mod config;
mod error;
mod models;
mod store;
mod services;
mod clients;
mod api;

use clients::PanSouClient;
use store::{NotificationStore, SettingsStore, SubscriptionStore};
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

    // 初始化 Stores
    let subscription_store = Arc::new(SubscriptionStore::new(config.data_dir.join("subscriptions.json")));
    subscription_store.load().await?;
    tracing::info!("✅ Loaded {} subscriptions", subscription_store.count().await);

    let settings_store = Arc::new(SettingsStore::new(config.data_dir.join("settings.json")));
    settings_store.load().await?;
    tracing::info!("✅ Settings loaded");

    let notification_store = Arc::new(NotificationStore::new(config.data_dir.join("notifications.json")));
    notification_store.load().await?;
    tracing::info!("✅ Loaded notifications");

    // 初始化客户端
    let pansou_client = Arc::new(PanSouClient::default());
    tracing::info!("✅ Clients initialized");

    // 创建应用
    let app = api::create_app(
        subscription_store,
        settings_store,
        notification_store,
        pansou_client,
    );

    // 绑定地址
    let addr = std::net::SocketAddr::from((
        config.server.host.parse::<std::net::IpAddr>()
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
