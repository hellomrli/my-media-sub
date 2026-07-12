use my_media_sub::app::AppContext;
use my_media_sub::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    my_media_sub::observability::init_tracing();
    tracing::info!("🦀 Starting my-media-sub Rust v2...");

    let config = my_media_sub::config::Config::load()?;
    tracing::info!("✅ Configuration loaded");
    tracing::info!("   Server: {}:{}", config.server.host, config.server.port);
    tracing::info!("   Data dir: {}", config.data_dir.display());

    let context = AppContext::new(&config).await?;
    context.start_background_services().await?;

    let job_queue = context.job_queue.clone();
    let app = my_media_sub::api::create_app(context);

    let addr = std::net::SocketAddr::from((
        config
            .server
            .host
            .parse::<std::net::IpAddr>()
            .unwrap_or_else(|_| std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0))),
        config.server.port,
    ));

    tracing::info!("🚀 Server starting on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("✅ Server listening on http://{}", addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    // HTTP 已停止接收新连接；关闭任务队列：拒绝新任务，给运行中任务一个
    // 有限宽限期到达持久化点，剩余任务收敛为可重试终态后落盘退出。
    tracing::info!("HTTP server stopped, shutting down job queue...");
    job_queue.shutdown().await;
    tracing::info!("Shutdown complete");

    Ok(())
}

/// 等待 Ctrl+C 或 SIGTERM（容器停止时 Docker 发送的信号）。
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!("监听 Ctrl+C 信号失败: {}", error);
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::warn!("监听 SIGTERM 信号失败: {}", error);
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("收到 Ctrl+C，开始优雅关闭"),
        _ = terminate => tracing::info!("收到 SIGTERM，开始优雅关闭"),
    }
}
