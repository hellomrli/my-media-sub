//! 在线升级重启的回归测试。
//!
//! 生产故障：点「更新」的页面自己持有 `/api/jobs/events` 的 SSE 连接，
//! axum 的优雅关闭要等所有在途连接结束，SSE 永远不会自己结束，于是进程
//! 停在「已不监听但不退出」的状态——新版本二进制已就位却从未被执行，
//! 容器重启策略也救不回来（进程还活着）。
//!
//! 本文件独占一个测试进程：关闭信号是进程级全局量。

use my_media_sub::{api::create_app, app::AppContext, config::Config};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const PASSWORD: &str = "graceful-shutdown-pw";

async fn test_context() -> (Arc<AppContext>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("my-media-sub-shutdown-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = Config {
        server: my_media_sub::config::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    };
    let context = AppContext::new(&config).await.expect("context init");
    context
        .settings_store
        .update(|settings| settings.app_password = PASSWORD.to_string())
        .await
        .expect("seed password");
    (context, dir)
}

#[tokio::test]
async fn open_sse_connection_does_not_block_restart_shutdown() {
    let (context, dir) = test_context().await;
    let app = create_app(context);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // 与 main.rs 相同的关闭编排：等待重启请求 → 广播关闭信号。
    let restart_requested = Arc::new(AtomicBool::new(false));
    let flag = restart_requested.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            my_media_sub::restart::wait_for_request().await;
            flag.store(true, Ordering::Release);
            my_media_sub::shutdown::begin();
        })
        .await
    });

    // 打开 SSE 长连接并读到首个 snapshot 事件，确保连接确实建立在服务端。
    let client = reqwest::Client::new();
    let mut sse = client
        .get(format!("http://{addr}/api/jobs/events"))
        .basic_auth("admin", Some(PASSWORD))
        .send()
        .await
        .expect("SSE 连接应当建立");
    assert!(sse.status().is_success());
    let first = tokio::time::timeout(Duration::from_secs(5), sse.chunk())
        .await
        .expect("首个事件应及时到达")
        .expect("读取 SSE 数据失败");
    let first = String::from_utf8_lossy(&first.expect("SSE 应先推送快照")).to_string();
    assert!(first.contains("snapshot"), "首个事件应为快照: {first}");

    // 触发在线升级重启：SSE 仍然开着。
    my_media_sub::restart::request(my_media_sub::restart::RestartPlan::for_executable(
        std::path::Path::new("/nonexistent-binary-for-test"),
    ))
    .expect("重启请求应被接受");

    // 修复前这里会永久挂起——优雅关闭在等一条永不结束的 SSE 连接。
    let served = tokio::time::timeout(Duration::from_secs(20), server)
        .await
        .expect("优雅关闭必须在 SSE 连接仍打开时完成")
        .expect("服务任务不应 panic");
    assert!(served.is_ok(), "服务应正常结束: {served:?}");
    assert!(restart_requested.load(Ordering::Acquire));

    let _ = std::fs::remove_dir_all(dir);
}
