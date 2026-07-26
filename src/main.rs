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
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_or_restart_signal(restart_requested.clone()));

    // 连接排空有硬上限。SSE 已经会在关闭信号后自行收尾，但任何赖着不走的连接
    // （慢客户端、代理中的大文件）都不能把在线升级重启永久卡在这里——那会让服务
    // 停在「已停止监听但进程不退出」的状态，容器重启策略也救不回来。
    tokio::select! {
        result = server => result?,
        _ = graceful_shutdown_deadline() => {
            tracing::warn!(
                "优雅关闭超过 {} 秒仍有连接未结束，强制继续关闭流程",
                GRACEFUL_SHUTDOWN_TIMEOUT.as_secs()
            );
        }
    }

    // HTTP 已停止接收新连接；关闭任务队列：拒绝新任务，给运行中任务一个
    // 有限宽限期到达持久化点，剩余任务收敛为可重试终态后落盘退出。
    tracing::info!("HTTP server stopped, shutting down job queue...");
    job_queue.shutdown().await;
    tracing::info!("Shutdown complete");

    if restart_requested.load(Ordering::Acquire) {
        if let Some(plan) = my_media_sub::restart::take_request() {
            tracing::info!("升级完成，正在启动新版本服务");
            // exec 成功时不会返回；失败必须退出进程，让 systemd / 容器重启策略
            // 拉起新版本，绝不能留下一个不再监听端口的存活进程。
            let error = my_media_sub::restart::execute(plan).unwrap_err();
            tracing::error!("启动新版本失败，进程退出交由外部拉起: {}", error);
            std::process::exit(1);
        }
        tracing::error!("已请求重启但没有可执行的重启计划，进程退出交由外部拉起");
        std::process::exit(1);
    }

    Ok(())
}

/// 收到关闭信号后允许连接排空的时长。
const GRACEFUL_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// 关闭开始后开始计时的兜底期限。
async fn graceful_shutdown_deadline() {
    my_media_sub::shutdown::wait().await;
    tokio::time::sleep(GRACEFUL_SHUTDOWN_TIMEOUT).await;
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
    // 通知 SSE 等长连接收尾，否则它们会一直挂着，优雅关闭永远等不到结束。
    my_media_sub::shutdown::begin();
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
