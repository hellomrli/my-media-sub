use my_media_sub::app::AppContext;
use my_media_sub::error::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    if version_requested() {
        println!("my-media-sub {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

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
    let restart_requested = Arc::new(AtomicBool::new(false));
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_or_restart_signal(restart_requested.clone()))
    .await?;

    // HTTP 已停止接收新连接；关闭任务队列：拒绝新任务，给运行中任务一个
    // 有限宽限期到达持久化点，剩余任务收敛为可重试终态后落盘退出。
    tracing::info!("HTTP server stopped, shutting down job queue...");
    job_queue.shutdown().await;
    tracing::info!("Shutdown complete");

    if restart_requested.load(Ordering::Acquire) {
        if let Some(plan) = my_media_sub::restart::take_request() {
            tracing::info!("升级完成，正在启动新版本服务");
            my_media_sub::restart::execute(plan)?;
            #[cfg(not(unix))]
            std::process::exit(0);
        }
    }

    Ok(())
}

fn version_requested() -> bool {
    let mut args = std::env::args_os().skip(1);
    matches!(
        (args.next().as_deref(), args.next()),
        (Some(value), None) if value == "--version" || value == "-V"
    )
}

async fn shutdown_or_restart_signal(restart_requested: Arc<AtomicBool>) {
    tokio::select! {
        _ = shutdown_signal() => {}
        _ = my_media_sub::restart::wait_for_request() => {
            restart_requested.store(true, Ordering::Release);
            tracing::info!("收到在线升级重启请求，开始优雅关闭");
        }
    }
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
