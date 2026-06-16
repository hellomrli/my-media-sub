use std::sync::Arc;

mod api;
mod clients;
mod config;
mod error;
mod models;
mod services;
mod store;

use clients::PanSouClient;
use error::Result;
use services::{SubscriptionCheckService, SubscriptionScheduler, SubscriptionTransferService};
use store::{NotificationStore, SettingsStore, SubscriptionStore};

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

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
    let subscription_store = Arc::new(SubscriptionStore::new(
        config.data_dir.join("subscriptions.json"),
    ));
    subscription_store.load().await?;
    tracing::info!(
        "✅ Loaded {} subscriptions",
        subscription_store.count().await
    );

    let settings_store = Arc::new(SettingsStore::new(config.data_dir.join("settings.json")));
    settings_store.load().await?;
    if [
        "APP_USERNAME",
        "SERVER_USERNAME",
        "APP_PASSWORD",
        "SERVER_PASSWORD",
        "QUARK_COOKIE",
        "WECOM_BOT_URL",
        "WXPUSHER_APP_TOKEN",
        "WXPUSHER_UIDS",
        "TELEGRAM_BOT_TOKEN",
        "TELEGRAM_CHAT_ID",
        "BARK_URL",
        "GOTIFY_URL",
        "GOTIFY_TOKEN",
        "PUSHPLUS_TOKEN",
        "SERVERCHAN_KEY",
    ]
    .iter()
    .any(|key| env_non_empty(key).is_some())
    {
        settings_store
            .update(|settings| {
                if let Some(value) =
                    env_non_empty("APP_USERNAME").or_else(|| env_non_empty("SERVER_USERNAME"))
                {
                    settings.app_username = value;
                }
                if let Some(value) =
                    env_non_empty("APP_PASSWORD").or_else(|| env_non_empty("SERVER_PASSWORD"))
                {
                    settings.app_password = value;
                }
                if let Some(value) = env_non_empty("QUARK_COOKIE") {
                    settings.quark_cookie = value;
                }
                if let Some(value) = env_non_empty("WECOM_BOT_URL") {
                    settings.wecom_bot_url = value;
                }
                if let Some(value) = env_non_empty("WXPUSHER_APP_TOKEN") {
                    settings.wxpusher_app_token = value;
                }
                if let Some(value) = env_non_empty("WXPUSHER_UIDS") {
                    settings.wxpusher_uids = value;
                }
                if let Some(value) = env_non_empty("TELEGRAM_BOT_TOKEN") {
                    settings.telegram_bot_token = value;
                }
                if let Some(value) = env_non_empty("TELEGRAM_CHAT_ID") {
                    settings.telegram_chat_id = value;
                }
                if let Some(value) = env_non_empty("BARK_URL") {
                    settings.bark_url = value;
                }
                if let Some(value) = env_non_empty("GOTIFY_URL") {
                    settings.gotify_url = value;
                }
                if let Some(value) = env_non_empty("GOTIFY_TOKEN") {
                    settings.gotify_token = value;
                }
                if let Some(value) = env_non_empty("PUSHPLUS_TOKEN") {
                    settings.pushplus_token = value;
                }
                if let Some(value) = env_non_empty("SERVERCHAN_KEY") {
                    settings.serverchan_key = value;
                }
            })
            .await?;
    }
    tracing::info!("✅ Settings loaded");

    let notification_store = Arc::new(NotificationStore::new(
        config.data_dir.join("notifications.json"),
    ));
    notification_store.load().await?;
    tracing::info!("✅ Loaded notifications");

    // 初始化客户端
    let pansou_client = Arc::new(PanSouClient::default());

    tracing::info!("✅ Clients initialized");

    // 初始化订阅服务
    let transfer_service = Arc::new(SubscriptionTransferService::new(
        subscription_store.clone(),
        settings_store.clone(),
        notification_store.clone(),
    ));

    let check_service = Arc::new(
        SubscriptionCheckService::new(
            subscription_store.clone(),
            settings_store.clone(),
            notification_store.clone(),
        )
        .with_transfer_service(transfer_service),
    );

    // 初始化调度器
    let scheduler =
        Arc::new(SubscriptionScheduler::new(check_service, settings_store.clone()).await?);

    // 启动调度器
    scheduler.start().await?;

    tracing::info!("✅ Services initialized");

    // 创建应用
    let app = api::create_app(
        subscription_store,
        settings_store,
        notification_store,
        pansou_client,
        scheduler,
    );

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
