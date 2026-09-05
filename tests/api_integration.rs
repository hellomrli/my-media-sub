/// HTTP 层集成测试：通过 axum 的 oneshot 机制直接在进程内发起请求，
/// 不启动真实 TCP 侦听器，快速验证路由、鉴权和 CRUD 行为。
use axum::{
    body::Body,
    http::{header, HeaderMap, Method, Request, StatusCode},
};
use base64::{engine::general_purpose, Engine};
use my_media_sub::{api::create_app, app::AppContext, config::Config};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tower::ServiceExt;

// ─── 测试辅助函数 ───────────────────────────────────────────────────────────

/// 创建使用临时目录的 AppContext（不启动后台调度器）
async fn test_context() -> (Arc<AppContext>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("my-media-sub-api-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();

    let config = Config {
        server: my_media_sub::config::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    };

    let context = AppContext::new(&config)
        .await
        .expect("test context init failed");
    // 配置里的密码只用于旧路径；设置存储默认仍是 "change-me"，而登录已拒绝默认密码，
    // 因此这里把测试密码写入设置存储，模拟运维已设置过密码的正常部署。
    context
        .settings_store
        .update(|settings| settings.app_password = "test-secret-pw".to_string())
        .await
        .expect("seed test password");
    (context, dir)
}

/// 生成 Basic Auth 头的值（base64("user:pass")）
fn basic_auth_header(user: &str, pass: &str) -> String {
    let encoded = general_purpose::STANDARD.encode(format!("{user}:{pass}"));
    format!("Basic {encoded}")
}

/// 向 app 发起单次请求，返回状态码
async fn status(app: &axum::Router, req: Request<Body>) -> StatusCode {
    app.clone().oneshot(req).await.unwrap().status()
}

/// 读取响应 body 为 JSON Value
async fn json_body(app: &axum::Router, req: Request<Body>) -> serde_json::Value {
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

async fn json_response(
    app: &axum::Router,
    req: Request<Body>,
) -> (StatusCode, HeaderMap, serde_json::Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, headers, body)
}

fn assert_json_content_type(headers: &HeaderMap) {
    assert!(headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json")));
}

/// 按顺序处理一组请求的本地 JSON-RPC 服务器，并把收到的 payload 交给测试断言。
async fn mock_json_rpc_sequence(
    results: Vec<serde_json::Value>,
) -> (String, oneshot::Receiver<Vec<serde_json::Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (payload_tx, payload_rx) = oneshot::channel();
    tokio::spawn(async move {
        let mut payloads = Vec::with_capacity(results.len());
        for result in results {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let (header_end, content_length) = loop {
                let mut chunk = [0u8; 4096];
                let read = socket.read(&mut chunk).await.unwrap();
                assert!(read > 0, "JSON-RPC 请求在 body 完整前结束");
                request.extend_from_slice(&chunk[..read]);

                let Some(header_end) = request
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .map(|position| position + 4)
                else {
                    continue;
                };
                let headers = std::str::from_utf8(&request[..header_end]).unwrap();
                let content_length = headers
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                    .expect("JSON-RPC 请求缺少 Content-Length");
                if request.len() >= header_end + content_length {
                    break (header_end, content_length);
                }
            };

            let payload = serde_json::from_slice::<serde_json::Value>(
                &request[header_end..header_end + content_length],
            )
            .unwrap();
            payloads.push(payload);

            let body = serde_json::json!({
                "jsonrpc": "2.0",
                "id": "my-media-sub",
                "result": result,
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.flush().await.unwrap();
        }
        let _ = payload_tx.send(payloads);
    });
    (format!("http://{address}/jsonrpc"), payload_rx)
}

// ─── /health ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_ok_without_auth() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let (status, headers, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_json_content_type(&headers);
    assert_eq!(body["status"], "ok");
    assert!(body.get("ok").is_none());
    let _ = std::fs::remove_dir_all(dir);
}

// ─── Basic Auth ───────────────────────────────────────────────────────────

#[tokio::test]
async fn protected_route_returns_401_without_credentials() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let req = Request::builder()
        .uri("/api/subscriptions")
        .body(Body::empty())
        .unwrap();

    let (status, headers, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_json_content_type(&headers);
    assert_eq!(
        headers.get(header::WWW_AUTHENTICATE).unwrap(),
        r#"Basic realm="my-media-sub""#
    );
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "unauthorized");
    assert!(body["message"]
        .as_str()
        .is_some_and(|value| !value.is_empty()));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn protected_route_returns_401_with_wrong_credentials() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let req = Request::builder()
        .uri("/api/subscriptions")
        .header(header::AUTHORIZATION, basic_auth_header("admin", "wrong"))
        .body(Body::empty())
        .unwrap();

    let (status, headers, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "unauthorized");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn protected_route_returns_200_with_correct_credentials() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let req = Request::builder()
        .uri("/api/subscriptions")
        .header(
            header::AUTHORIZATION,
            basic_auth_header("admin", "test-secret-pw"),
        )
        .body(Body::empty())
        .unwrap();

    assert_eq!(status(&app, req).await, StatusCode::OK);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn api_success_responses_use_the_shared_envelope() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let subscriptions = json_body(&app, auth_get("/api/subscriptions")).await;
    assert_eq!(subscriptions["ok"], true);
    assert!(subscriptions["data"].is_array());

    // Empty Quark configuration still returns a successful, typed drive payload.
    let drive = json_body(&app, auth_get("/api/drive?fid=0")).await;
    assert_eq!(drive["ok"], true);
    assert!(drive["data"]["list"].is_array());

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn aria2_purge_route_calls_the_destructive_rpc_method() {
    let (ctx, dir) = test_context().await;
    let (rpc_url, payload_rx) =
        mock_json_rpc_sequence(vec![serde_json::json!([]), serde_json::json!("OK")]).await;
    ctx.settings_store
        .update(|settings| {
            settings.aria2_rpc_url = rpc_url;
            settings.aria2_secret = "rpc-secret".to_string();
        })
        .await
        .unwrap();
    let app = create_app(ctx);

    let (status, headers, body) = json_response(
        &app,
        auth_post("/api/drive/aria2/tasks/purge-all", serde_json::json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["success"], true);
    assert_eq!(body["data"]["message"], "已清空 0 条已停止的下载记录");

    let payloads = tokio::time::timeout(std::time::Duration::from_secs(1), payload_rx)
        .await
        .expect("Aria2 mock 未在期限内收到 RPC 请求")
        .unwrap();
    assert_eq!(payloads[0]["method"], "aria2.tellStopped");
    assert_eq!(payloads[0]["params"][1], -1);
    assert_eq!(payloads[0]["params"][2], 1_000);
    assert_eq!(payloads[1]["method"], "aria2.purgeDownloadResult");
    assert_eq!(
        payloads[1]["params"],
        serde_json::json!(["token:rpc-secret"])
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn aria2_tasks_return_the_latest_thousand_stopped_results() {
    let (ctx, dir) = test_context().await;
    let (rpc_url, payload_rx) = mock_json_rpc_sequence(vec![
        serde_json::json!([]),
        serde_json::json!([]),
        serde_json::json!([{"gid":"latest","status":"error"}]),
    ])
    .await;
    ctx.settings_store
        .update(|settings| {
            settings.aria2_rpc_url = rpc_url;
            settings.aria2_secret = "rpc-secret".to_string();
        })
        .await
        .unwrap();
    let app = create_app(ctx);

    let body = json_body(&app, auth_get("/api/drive/aria2/tasks?stopped_limit=50000")).await;
    assert_eq!(body["data"]["stopped"][0]["gid"], "latest");

    let payloads = tokio::time::timeout(std::time::Duration::from_secs(1), payload_rx)
        .await
        .expect("Aria2 mock 未在期限内收到任务列表 RPC 请求")
        .unwrap();
    assert_eq!(payloads[0]["method"], "aria2.tellActive");
    assert_eq!(payloads[1]["method"], "aria2.tellWaiting");
    assert_eq!(payloads[2]["method"], "aria2.tellStopped");
    assert_eq!(payloads[2]["params"][1], -1);
    assert_eq!(payloads[2]["params"][2], 1_000);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn aria2_fast_poll_skips_the_stopped_results_rpc() {
    let (ctx, dir) = test_context().await;
    let (rpc_url, payload_rx) =
        mock_json_rpc_sequence(vec![serde_json::json!([]), serde_json::json!([])]).await;
    ctx.settings_store
        .update(|settings| settings.aria2_rpc_url = rpc_url)
        .await
        .unwrap();
    let app = create_app(ctx);

    let body = json_body(&app, auth_get("/api/drive/aria2/tasks?stopped_limit=0")).await;
    assert_eq!(body["data"]["stopped"], serde_json::json!([]));

    let payloads = tokio::time::timeout(std::time::Duration::from_secs(1), payload_rx)
        .await
        .expect("Aria2 mock 未在期限内收到快速轮询 RPC 请求")
        .unwrap();
    assert_eq!(payloads.len(), 2);
    assert_eq!(payloads[0]["method"], "aria2.tellActive");
    assert_eq!(payloads[1]["method"], "aria2.tellWaiting");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn aria2_single_task_route_queries_only_the_disappeared_gid() {
    let (ctx, dir) = test_context().await;
    let (rpc_url, payload_rx) = mock_json_rpc_sequence(vec![serde_json::json!({
        "gid": "settled-1",
        "status": "error",
        "errorMessage": "network failed"
    })])
    .await;
    ctx.settings_store
        .update(|settings| {
            settings.aria2_rpc_url = rpc_url;
            settings.aria2_secret = "rpc-secret".to_string();
        })
        .await
        .unwrap();
    let app = create_app(ctx);

    let body = json_body(&app, auth_get("/api/drive/aria2/tasks/settled-1")).await;
    assert_eq!(body["data"]["gid"], "settled-1");
    assert_eq!(body["data"]["status"], "error");

    let payloads = tokio::time::timeout(std::time::Duration::from_secs(1), payload_rx)
        .await
        .expect("Aria2 mock 未在期限内收到单任务 RPC 请求")
        .unwrap();
    assert_eq!(payloads.len(), 1);
    assert_eq!(payloads[0]["method"], "aria2.tellStatus");
    assert_eq!(payloads[0]["params"][1], "settled-1");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn job_events_remain_an_sse_response() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let response = app
        .clone()
        .oneshot(auth_get("/api/jobs/events"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream")));

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn static_javascript_is_not_wrapped_as_an_api_response() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let response = app
        .clone()
        .oneshot(auth_get("/js/core/api.js"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/javascript")));

    let _ = std::fs::remove_dir_all(dir);
}

// ─── CSRF 防护 ────────────────────────────────────────────────────────────

#[tokio::test]
async fn cross_site_post_returns_403() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/subscriptions")
        .header(header::ORIGIN, "https://evil.example.com")
        .header(header::HOST, "media.internal.com")
        .header(
            header::AUTHORIZATION,
            basic_auth_header("admin", "test-secret-pw"),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let (status, headers, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "csrf_forbidden");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn sec_fetch_site_cross_site_post_returns_403() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/subscriptions")
        .header("sec-fetch-site", "cross-site")
        .header(
            header::AUTHORIZATION,
            basic_auth_header("admin", "test-secret-pw"),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let (status, headers, body) = json_response(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "csrf_forbidden");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn unknown_api_route_returns_json_404() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let (status, headers, body) = json_response(&app, auth_get("/api/no-such-route")).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "not_found");

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn malformed_json_rejection_uses_the_error_envelope() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/subscriptions")
        .header(
            header::AUTHORIZATION,
            basic_auth_header("admin", "test-secret-pw"),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{"))
        .unwrap();
    let (status, headers, body) = json_response(&app, req).await;

    assert!(matches!(
        status,
        StatusCode::BAD_REQUEST | StatusCode::BAD_REQUEST
    ));
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], false);
    assert!(matches!(
        body["error"].as_str(),
        Some("bad_request" | "invalid_request")
    ));

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn method_not_allowed_rejection_uses_the_error_envelope() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let req = Request::builder()
        .method(Method::PUT)
        .uri("/api/jobs")
        .header(
            header::AUTHORIZATION,
            basic_auth_header("admin", "test-secret-pw"),
        )
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = json_response(&app, req).await;

    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "method_not_allowed");
    assert!(headers.contains_key(header::ALLOW));

    let _ = std::fs::remove_dir_all(dir);
}

// ─── 订阅 CRUD ────────────────────────────────────────────────────────────

fn auth_get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(
            header::AUTHORIZATION,
            basic_auth_header("admin", "test-secret-pw"),
        )
        .body(Body::empty())
        .unwrap()
}

fn auth_post(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(
            header::AUTHORIZATION,
            basic_auth_header("admin", "test-secret-pw"),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn auth_put(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(Method::PUT)
        .uri(uri)
        .header(
            header::AUTHORIZATION,
            basic_auth_header("admin", "test-secret-pw"),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn auth_delete(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::DELETE)
        .uri(uri)
        .header(
            header::AUTHORIZATION,
            basic_auth_header("admin", "test-secret-pw"),
        )
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn list_subscriptions_returns_empty_array_initially() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let body = json_body(&app, auth_get("/api/subscriptions")).await;
    let items = body["data"].as_array().expect("data should be array");
    assert!(items.is_empty(), "new store should have no subscriptions");

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn tmdb_image_proxy_rejects_untrusted_paths_before_network_access() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);
    let (status, headers, body) =
        json_response(&app, auth_get("/api/images/tmdb/w999/poster.svg")).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "validation_error");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn create_subscription_returns_201_and_can_be_fetched() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let payload = serde_json::json!({
        "title": "API Test Series",
        "url": "https://pan.quark.cn/s/api-test-001",
        "media_type": "series",
        "season": 1
    });

    let resp = app
        .clone()
        .oneshot(auth_post("/api/subscriptions", payload))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let id = created["data"]["id"]
        .as_str()
        .expect("created sub should have id");
    assert!(!id.is_empty());

    // 用 GET 能取回
    let list = json_body(&app, auth_get("/api/subscriptions")).await;
    let items = list["data"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str().unwrap(), id);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn create_subscription_cleans_quality_suffix_before_persisting_title() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let created = json_body(
        &app,
        auth_post(
            "/api/subscriptions",
            serde_json::json!({
                "title": "凡人修仙传 4K 高码率",
                "url": "https://pan.quark.cn/s/title-normalize-001",
                "media_type": "series",
                "season": 1
            }),
        ),
    )
    .await;

    assert_eq!(created["data"]["title"], "凡人修仙传");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn changing_subscription_season_preserves_progress_and_reopens() {
    // 「暂不转存」语义：调整季度范围是编辑扩季/缩季的常规操作，不得清空
    // 已转存/已知进度（旧行为整体重置，扩季后会把已转存的季重复转存）。
    // 按季保留历史证据；新季计数与完结目标不沿用旧季。
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx.clone());
    let created = json_body(
        &app,
        auth_post(
            "/api/subscriptions",
            serde_json::json!({
                "title": "Season Reset",
                "url": "https://pan.quark.cn/s/season-reset",
                "media_type": "series",
                "season": 1
            }),
        ),
    )
    .await;
    let id = created["data"]["id"].as_str().unwrap().to_string();
    ctx.subscription_store
        .update(&id, |subscription| {
            subscription.current_episode_number = 12;
            subscription.total_episode_number = Some(12);
            subscription.known_files = vec!["Show.S01E12.mkv".to_string()];
            subscription.known_episodes = vec![12];
            subscription.transferred_files = vec!["Show.S01E12.mkv".to_string()];
            subscription.transferred_file_keys = vec!["ep:12".to_string()];
            subscription.sync_downloads = vec![my_media_sub::models::SyncDownloadRecord {
                gid: "gid-12".to_string(),
                file_name: "Show.S01E12.mkv".to_string(),
                download_dir: "/downloads".to_string(),
                target_dir: "/series/Show/Season 1".to_string(),
                submitted_at: 1,
                completed_at: Some(2),
            }];
            subscription.completed = true;
            subscription.status = "completed".to_string();
        })
        .await
        .unwrap();

    let updated = json_body(
        &app,
        auth_put(
            &format!("/api/subscriptions/{id}"),
            serde_json::json!({"season": 2}),
        ),
    )
    .await;

    assert_eq!(updated["data"]["season"], 2);
    // 历史证据保留，但 S1E12 不能算作 S2 的当前进度。
    assert_eq!(updated["data"]["current_episode_number"], 0);
    assert!(updated["data"]["known_episodes"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(updated["data"]["transferred_file_keys"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("s:1:ep:12")));
    assert_eq!(updated["data"]["known_files"].as_array().unwrap().len(), 1);
    assert_eq!(
        updated["data"]["transferred_files"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(updated["data"]
        .get("sync_downloads")
        .is_some_and(|value| value.as_array().is_some_and(|items| !items.is_empty())));
    // 完结释放：S1 的旧完结目标不再适用，订阅回到追更中
    assert!(updated["data"]["total_episode_number"].is_null());
    assert_eq!(updated["data"]["completed"], false);
    assert_eq!(updated["data"]["status"], "active");

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn create_subscription_with_duplicate_url_returns_validation_error() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let payload = serde_json::json!({
        "title": "Dup Test",
        "url": "https://pan.quark.cn/s/dup-test",
        "media_type": "series",
        "season": 1
    });

    let s1 = app
        .clone()
        .oneshot(auth_post("/api/subscriptions", payload.clone()))
        .await
        .unwrap()
        .status();
    assert_eq!(s1, StatusCode::CREATED);

    let response = app
        .clone()
        .oneshot(auth_post("/api/subscriptions", payload))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let error: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(error["ok"], false);
    assert_eq!(error["error"], "validation_error");
    assert!(error["message"]
        .as_str()
        .is_some_and(|message| !message.is_empty()));

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn delete_subscription_returns_204() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let payload = serde_json::json!({
        "title": "Delete Me",
        "url": "https://pan.quark.cn/s/delete-me",
        "media_type": "movie",
        "season": 1
    });

    let resp = app
        .clone()
        .oneshot(auth_post("/api/subscriptions", payload))
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let delete_response = app
        .clone()
        .oneshot(auth_delete(&format!(
            "/api/subscriptions/{id}?confirm={id}"
        )))
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);
    let bytes = axum::body::to_bytes(delete_response.into_body(), 1 << 20)
        .await
        .unwrap();
    assert!(bytes.is_empty());

    // 再 GET 确认已消失
    let list = json_body(&app, auth_get("/api/subscriptions")).await;
    assert!(list["data"].as_array().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn get_nonexistent_subscription_returns_404() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let (status, headers, body) =
        json_response(&app, auth_get("/api/subscriptions/no-such-id")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "not_found");

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn subscription_status_returns_episode_aggregation() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx.clone());

    let payload = serde_json::json!({
        "title": "Status Test Series",
        "url": "https://pan.quark.cn/s/status-test",
        "media_type": "series",
        "season": 1,
        "rules": {"finish_after_episode": 6}
    });
    let resp = app
        .clone()
        .oneshot(auth_post("/api/subscriptions", payload))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let id = created["data"]["id"].as_str().unwrap().to_string();

    ctx.subscription_store
        .update(&id, |subscription| {
            subscription.current_episode_number = 4;
            subscription.total_episode_number = Some(6);
            subscription.known_episodes = vec![1, 2, 4];
            subscription.known_files = vec![
                "Show.S01E01.mkv".to_string(),
                "Show.S01E02.mkv".to_string(),
                "Show.S01E04.mkv".to_string(),
            ];
            subscription.transferred_files = vec!["Show.S01E01.mkv".to_string()];
            subscription.transferred_file_keys = vec!["ep:1".to_string()];
        })
        .await
        .unwrap();

    let body = json_body(&app, auth_get(&format!("/api/subscriptions/{id}/status"))).await;
    assert_eq!(body["data"]["summary"]["expected_count"], 6);
    assert_eq!(body["data"]["summary"]["discovered_count"], 3);
    assert_eq!(body["data"]["summary"]["transferred_count"], 1);
    assert_eq!(body["data"]["summary"]["latest_discovered_episode"], 4);
    assert_eq!(body["data"]["summary"]["latest_transferred_episode"], 1);
    assert_eq!(
        body["data"]["missing_episodes"],
        serde_json::json!([3, 5, 6])
    );
    assert_eq!(body["data"]["episodes"].as_array().unwrap().len(), 6);
    assert_eq!(body["data"]["pipeline"].as_array().unwrap().len(), 6);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn calendar_returns_metadata_schedule_with_summary_and_actions() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx.clone());

    let payload = serde_json::json!({
        "title": "Calendar Test Series",
        "url": "https://pan.quark.cn/s/calendar-test",
        "media_type": "series",
        "season": 1,
        "metadata": {
            "provider": "tmdb",
            "provider_id": "1",
            "title": "Calendar Test Series",
            "media_type": "series",
            "number_of_episodes": 4,
            "episodes": [
                {"season_number": 1, "episode_number": 1, "air_date": "2026-07-06"},
                {"season_number": 1, "episode_number": 2, "air_date": "2026-07-09"},
                {"season_number": 1, "episode_number": 3, "air_date": "2026-07-13"},
                {"season_number": 1, "episode_number": 4, "air_date": "2026-07-16"}
            ]
        }
    });
    let response = app
        .clone()
        .oneshot(auth_post("/api/subscriptions", payload))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let id = created["data"]["id"].as_str().unwrap().to_string();
    ctx.subscription_store
        .update(&id, |subscription| {
            subscription.known_episodes = vec![1, 2];
            subscription.transferred_file_keys = vec!["ep:1".to_string()];
            // Alert tests set this explicitly. Keeping it zero here makes the
            // API shape test independent from the wall-clock date.
            subscription.last_checked_at = 0;
        })
        .await
        .unwrap();

    let (status, headers, body) = json_response(
        &app,
        auth_get("/api/calendar?from=2026-07-06&to=2026-07-19&media_type=series"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["timezone"], "Asia/Shanghai");
    assert_eq!(body["data"]["summary"]["total"], 4);
    assert_eq!(body["data"]["summary"]["subscriptions"], 1);
    assert_eq!(body["data"]["summary"]["source_alerts"], 0);
    assert!(body["data"]["source_alerts"].as_array().unwrap().is_empty());
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items[0]["scheduled_date"], "2026-07-06");
    assert_eq!(items[0]["schedule_source"], "metadata_episode");
    assert_eq!(items[0]["latest_discovered_episode"], 2);
    assert_eq!(items[0]["latest_transferred_episode"], 1);
    assert_eq!(items[0]["source_change_recommended"], false);
    assert_eq!(items[0]["actions"]["can_check"], true);
    assert_eq!(items[0]["actions"]["can_switch_source"], true);
    assert!(items[0]["actions"]["detail_url"]
        .as_str()
        .unwrap()
        .contains("subscription="));

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn calendar_rejects_inverted_query_range() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let (status, headers, body) = json_response(
        &app,
        auth_get("/api/calendar?from=2026-07-10&to=2026-07-01"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "validation_error");

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn source_switch_preview_apply_history_and_rollback_are_safe() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx.clone());
    let create = json_body(
        &app,
        auth_post(
            "/api/subscriptions",
            serde_json::json!({
                "title": "Source Switch Series",
                "url": "https://pan.quark.cn/s/original-source",
                "password": "old-password",
                "media_type": "series",
                "season": 1
            }),
        ),
    )
    .await;
    let id = create["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().timestamp();
    ctx.subscription_store
        .update(&id, |subscription| {
            subscription.current_episode_number = 3;
            subscription.start_episode_number = Some(4);
            subscription.known_episodes = vec![1, 2, 3];
            subscription.transferred_file_keys = vec!["ep:1".to_string()];
            subscription.source_failure_count = 2;
            subscription.source_candidates =
                vec![my_media_sub::models::subscription::SourceCandidate {
                    id: "candidate-safe".to_string(),
                    source: "fixture".to_string(),
                    url: "https://pan.quark.cn/s/candidate-safe".to_string(),
                    password: "new-password".to_string(),
                    note: "Source Switch Series S01 2160P HDR H265".to_string(),
                    discovered_at: now,
                    probe_info: Some(my_media_sub::models::subscription::ProbeResult {
                        ok: true,
                        state: "success".to_string(),
                        message: "fixture".to_string(),
                        files: vec![my_media_sub::models::subscription::ProbeFile {
                            name: "Show.S01E04.2160p.HDR.HEVC.mkv".to_string(),
                            is_dir: false,
                            parent_path: "Season 1".to_string(),
                            size: 4_000_000_000,
                            updated_at: Some(chrono::Utc::now().to_rfc3339()),
                            file_key: "ep4".to_string(),
                        }],
                    }),
                    quality: my_media_sub::models::SourceQuality::default(),
                }];
        })
        .await
        .unwrap();

    let preview = json_body(
        &app,
        auth_post(
            &format!("/api/subscriptions/{id}/source-candidates/preview"),
            serde_json::json!({"candidate_id": "candidate-safe"}),
        ),
    )
    .await;
    assert_eq!(preview["ok"], true);
    assert_eq!(preview["data"]["probe_ok"], true);
    assert_eq!(preview["data"]["season_matches"], true);
    assert_eq!(preview["data"]["covers_progress"], true);
    assert_eq!(preview["data"]["can_apply"], true);
    assert!(
        preview["data"]["candidate"]["quality"]["score"]
            .as_u64()
            .unwrap()
            >= 85
    );

    let applied = json_body(
        &app,
        auth_post(
            &format!("/api/subscriptions/{id}/source-candidates/apply"),
            serde_json::json!({"candidate_id": "candidate-safe"}),
        ),
    )
    .await;
    assert_eq!(applied["ok"], true);
    assert_eq!(applied["data"]["success"], true);
    let switched = ctx.subscription_store.get(&id).await.unwrap();
    assert_eq!(switched.url, "https://pan.quark.cn/s/candidate-safe");
    assert_eq!(switched.known_episodes, vec![1, 2, 3]);
    assert_eq!(switched.transferred_file_keys, vec!["ep:1"]);
    assert_eq!(switched.source_switch_history.len(), 1);

    let history = json_body(
        &app,
        auth_get(&format!("/api/subscriptions/{id}/source-history")),
    )
    .await;
    assert_eq!(history["ok"], true);
    assert_eq!(history["data"][0]["status"], "succeeded");

    let rollback = json_body(
        &app,
        auth_post(
            &format!("/api/subscriptions/{id}/source-history/rollback"),
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(rollback["ok"], true);
    assert_eq!(rollback["data"]["success"], true);
    let restored = ctx.subscription_store.get(&id).await.unwrap();
    assert_eq!(restored.url, "https://pan.quark.cn/s/original-source");
    assert_eq!(restored.password, "old-password");
    assert_eq!(restored.known_episodes, vec![1, 2, 3]);
    assert_eq!(restored.transferred_file_keys, vec!["ep:1"]);
    assert_eq!(restored.source_switch_history[0].status, "rolled_back");

    let _ = std::fs::remove_dir_all(dir);
}

// ─── 设置读写 ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_settings_returns_current_values() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let body = json_body(&app, auth_get("/api/settings")).await;
    // 默认用户名应为 admin
    assert_eq!(body["data"]["app_username"].as_str().unwrap(), "admin");
    // 媒体库元数据落盘开关默认关闭
    assert_eq!(body["data"]["media_metadata_files_enabled"], false);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn media_metadata_files_enabled_setting_round_trips() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    // 开启后 GET 往返确认持久化。
    let (status, _, body) = json_response(
        &app,
        auth_post(
            "/api/settings",
            serde_json::json!({"media_metadata_files_enabled": true}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["media_metadata_files_enabled"], true);

    let body = json_body(&app, auth_get("/api/settings")).await;
    assert_eq!(body["data"]["media_metadata_files_enabled"], true);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn settings_reject_credentials_that_would_lock_out_login() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    for payload in [
        serde_json::json!({"app_password": "change-me"}),
        serde_json::json!({"app_username": ""}),
        serde_json::json!({"app_username": "   "}),
        serde_json::json!({"app_username": "user:name"}),
    ] {
        let (status, _, body) = json_response(&app, auth_post("/api/settings", payload)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");
        assert_eq!(body["ok"], false);
    }

    // 被拒绝的写入不能改变已保存的凭据：原密码仍然可以登录。
    let ok = status(&app, auth_get("/api/diagnostics")).await;
    assert_eq!(ok, StatusCode::OK);

    // 合法更新会去除用户名首尾空白并正常保存。
    let (status, _, body) = json_response(
        &app,
        auth_post(
            "/api/settings",
            serde_json::json!({"app_username": "  new-admin  "}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["app_username"], "new-admin");

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn source_switch_policy_settings_are_compatible_and_clamped() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);
    let updated = json_body(
        &app,
        auth_post(
            "/api/settings",
            serde_json::json!({
                "auto_source_switch_enabled": true,
                "auto_source_switch_mode": "apply",
                "source_switch_min_score": 150,
                "source_switch_min_score_delta": -5,
                "source_switch_failure_threshold": 0,
                "source_switch_cooldown_hours": 9999
            }),
        ),
    )
    .await;
    assert_eq!(updated["ok"], true);
    assert_eq!(updated["data"]["auto_source_switch_enabled"], true);
    assert_eq!(updated["data"]["auto_source_switch_mode"], "apply");
    assert_eq!(updated["data"]["source_switch_min_score"], 100);
    assert_eq!(updated["data"]["source_switch_min_score_delta"], 0);
    assert_eq!(updated["data"]["source_switch_failure_threshold"], 1);
    assert_eq!(updated["data"]["source_switch_cooldown_hours"], 720);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn automation_event_pipeline_summary_and_safe_retry_are_structured() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx.clone());
    let create = json_body(
        &app,
        auth_post(
            "/api/subscriptions",
            serde_json::json!({
                "title": "Automation Event Series",
                "url": "https://pan.quark.cn/s/events",
                "media_type": "series",
                "season": 1
            }),
        ),
    )
    .await;
    let subscription_id = create["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().timestamp();
    let mut source = my_media_sub::models::AutomationEvent::new(
        "event-source",
        "correlation-1",
        my_media_sub::models::AutomationStage::SourceCheck,
        my_media_sub::models::AutomationStatus::Succeeded,
        now,
    );
    source.subscription_id = Some(subscription_id.clone());
    source.message = "source ok".to_string();
    ctx.automation_event_store.add(source).await.unwrap();

    let mut failed = my_media_sub::models::AutomationEvent::new(
        "event-filter",
        "correlation-1",
        my_media_sub::models::AutomationStage::FileFilter,
        my_media_sub::models::AutomationStatus::Failed,
        now + 1,
    );
    failed.subscription_id = Some(subscription_id.clone());
    failed.episode = Some(4);
    failed.message = "filter failed".to_string();
    failed.error = "fixture failure".to_string();
    ctx.automation_event_store.add(failed).await.unwrap();

    let pipeline = json_body(
        &app,
        auth_get(&format!("/api/subscriptions/{subscription_id}/pipeline")),
    )
    .await;
    assert_eq!(pipeline["ok"], true);
    assert_eq!(pipeline["data"]["events"].as_array().unwrap().len(), 2);
    assert_eq!(
        pipeline["data"]["latest_by_stage"]["file_filter"]["status"],
        "failed"
    );
    assert_eq!(
        pipeline["data"]["episodes"]["4"][0]["error"],
        "fixture failure"
    );

    let mut other_episode = my_media_sub::models::AutomationEvent::new(
        "event-filter-episode-5",
        "correlation-2",
        my_media_sub::models::AutomationStage::FileFilter,
        my_media_sub::models::AutomationStatus::Succeeded,
        now + 2,
    );
    other_episode.subscription_id = Some(subscription_id.clone());
    other_episode.episode = Some(5);
    ctx.automation_event_store.add(other_episode).await.unwrap();
    let episode_pipeline = json_body(
        &app,
        auth_get(&format!(
            "/api/subscriptions/{subscription_id}/pipeline?episode=4"
        )),
    )
    .await;
    assert!(episode_pipeline["data"]["episodes"].get("4").is_some());
    assert!(episode_pipeline["data"]["episodes"].get("5").is_none());

    let summary = json_body(&app, auth_get("/api/automation/summary")).await;
    assert_eq!(summary["ok"], true);
    assert_eq!(summary["data"]["by_status"]["failed"], 1);
    assert_eq!(summary["data"]["recent_failed"][0]["id"], "event-filter");

    let job = my_media_sub::jobs::Job {
        id: "failed-job-for-retry".to_string(),
        kind: my_media_sub::jobs::JobKind::MetadataScrape,
        request_id: Some("request-test".to_string()),
        correlation_id: Some("correlation-test".to_string()),
        subscription_id: Some(subscription_id.to_string()),
        priority: my_media_sub::jobs::JobPriority::Low,
        attempt: 1,
        next_attempt_at: None,
        error_class: None,
        status: my_media_sub::jobs::JobStatus::Failed,
        progress: 100,
        title: "failed".to_string(),
        message: "failed".to_string(),
        idempotency_key: None,
        payload: serde_json::json!({"subscription_id": subscription_id, "overwrite": false}),
        result: None,
        error: Some("failed".to_string()),
        created_at: now,
        updated_at: now,
        started_at: Some(now),
        finished_at: Some(now),
    };
    ctx.job_store.add(job).await.unwrap();
    let mut retryable = my_media_sub::models::AutomationEvent::new(
        "event-job-failed",
        "correlation-job",
        my_media_sub::models::AutomationStage::VersionSelect,
        my_media_sub::models::AutomationStatus::Failed,
        now,
    );
    retryable.job_id = Some("failed-job-for-retry".to_string());
    ctx.automation_event_store.add(retryable).await.unwrap();
    let retried = json_body(
        &app,
        auth_post(
            "/api/automation/events/event-job-failed/retry",
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(retried["ok"], true);
    assert_eq!(retried["data"]["success"], true);
    assert!(retried["data"]["new_job_id"].as_str().is_some());
    assert_eq!(retried["data"]["event"]["status"], "retrying");
    assert_eq!(retried["data"]["event"]["attempt"], 2);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn security_headers_and_request_ids_are_present() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/diagnostics")
                .header(
                    header::AUTHORIZATION,
                    basic_auth_header("admin", "test-secret-pw"),
                )
                .header("x-request-id", "request-test-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "request-test-1"
    );
    assert_eq!(
        response.headers().get("x-correlation-id").unwrap(),
        "request-test-1"
    );
    assert!(response.headers().get("content-security-policy").is_some());
    assert_eq!(response.headers().get("x-frame-options").unwrap(), "DENY");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn backup_restore_is_confirmed_snapshotted_and_rejects_tampering() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);
    let export = app
        .clone()
        .oneshot(auth_get("/api/backups/export"))
        .await
        .unwrap();
    let archive_bytes = axum::body::to_bytes(export.into_body(), 16 << 20)
        .await
        .unwrap();
    let archive: serde_json::Value = serde_json::from_slice(&archive_bytes).unwrap();

    let wrong = auth_post(
        "/api/backups/restore",
        serde_json::json!({
            "archive": archive.clone(), "confirmation": "RESTORE"
        }),
    );
    assert_eq!(status(&app, wrong).await, StatusCode::BAD_REQUEST);
    assert!(!dir.join("restart-required.json").exists());

    let mut tampered = archive.clone();
    tampered["files"][0]["sha256"] = serde_json::Value::String("00".repeat(32));
    let preview = json_response(&app, auth_post("/api/backups/preview", tampered)).await;
    assert_eq!(preview.0, StatusCode::BAD_REQUEST);
    assert!(!dir.join("restart-required.json").exists());

    let restored = json_body(
        &app,
        auth_post(
            "/api/backups/restore",
            serde_json::json!({
                "archive": archive, "confirmation": "RESTORE DATA"
            }),
        ),
    )
    .await;
    assert_eq!(restored["ok"], true);
    assert_eq!(restored["data"]["restart_required"], true);
    assert!(dir.join("restart-required.json").exists());
    let snapshot = restored["data"]["snapshot"].as_str().unwrap();
    assert!(dir.join("backups").join(snapshot).is_file());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn repeated_auth_failures_are_rate_limited() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);
    for _ in 0..5 {
        let request = Request::builder()
            .uri("/api/subscriptions")
            .header(header::AUTHORIZATION, basic_auth_header("admin", "wrong"))
            .header("x-forwarded-for", "192.0.2.10")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status(&app, request).await, StatusCode::UNAUTHORIZED);
    }
    let request = Request::builder()
        .uri("/api/subscriptions")
        .header(header::AUTHORIZATION, basic_auth_header("admin", "wrong"))
        .header("x-forwarded-for", "192.0.2.10")
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = json_response(&app, request).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(headers.get(header::RETRY_AFTER).unwrap(), "60");
    assert_eq!(body["error"], "auth_rate_limited");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn backup_export_preview_and_diagnostics_are_available() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);
    let export = app
        .clone()
        .oneshot(auth_get("/api/backups/export"))
        .await
        .unwrap();
    assert_eq!(export.status(), StatusCode::OK);
    assert!(export.headers().get(header::CONTENT_DISPOSITION).is_some());
    let archive_bytes = axum::body::to_bytes(export.into_body(), 16 << 20)
        .await
        .unwrap();
    let archive: serde_json::Value = serde_json::from_slice(&archive_bytes).unwrap();
    assert_eq!(archive["format"], "my-media-sub-backup");
    assert!(archive["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["path"] == "telegram_bot.json"));

    let preview = json_body(&app, auth_post("/api/backups/preview", archive)).await;
    assert_eq!(preview["ok"], true);
    assert_eq!(preview["data"]["valid"], true);
    assert!(preview["data"]["file_count"].as_u64().unwrap() >= 1);
    assert!(preview["data"]["checks"].as_array().unwrap().len() >= 5);

    let created_backup = json_body(&app, auth_post("/api/backups", serde_json::json!({}))).await;
    assert_eq!(created_backup["ok"], true);
    let verification = json_body(&app, auth_get("/api/backups/verification")).await;
    assert_eq!(verification["data"]["status"], "passed");
    assert!(verification["data"]["restored_files"].as_u64().is_some());
    let reverified = json_body(
        &app,
        auth_post("/api/backups/verification", serde_json::json!({})),
    )
    .await;
    assert_eq!(reverified["data"]["status"], "passed");

    let diagnostics = json_body(&app, auth_get("/api/diagnostics")).await;
    assert_eq!(diagnostics["ok"], true);
    assert_eq!(diagnostics["data"]["schema_version"], 1);
    assert_eq!(
        diagnostics["data"]["storage_decision"]["recommendation"],
        "keep_json"
    );
    assert!(diagnostics["data"]["metrics"]["store_io"].is_object());
    assert!(diagnostics["data"]["metrics"]["external_dependencies"].is_object());
    assert!(
        diagnostics["data"]["environment"]["filesystem"]["data_dir_exists"]
            .as_bool()
            .unwrap()
    );
    assert!(diagnostics["data"]["environment"]["data_consistency"].is_array());
    assert!(diagnostics["data"]["recommendations"].is_array());
    assert_eq!(
        diagnostics["data"]["backups"]["latest_verification"]["status"],
        "passed"
    );
    assert!(diagnostics.to_string().find("test-secret-pw").is_none());

    let lifecycle = json_body(&app, auth_get("/api/storage/cleanup")).await;
    assert_eq!(lifecycle["ok"], true);
    assert_eq!(lifecycle["data"]["mutates_data"], false);
    assert_eq!(lifecycle["data"]["execution_requires"], "CLEANUP DATA");
    assert_eq!(
        lifecycle["data"]["sqlite_decision"]["runtime_backend"],
        "json"
    );
    assert_eq!(
        lifecycle["data"]["sqlite_decision"]["dual_write_active"],
        false
    );
    assert!(lifecycle["data"]["stores"].as_array().unwrap().len() >= 5);
    let decision = json_body(&app, auth_get("/api/storage/decision")).await;
    assert_eq!(decision["data"]["migration_phase"], "not_started");

    let compacted = json_body(
        &app,
        auth_post(
            "/api/storage/cleanup",
            serde_json::json!({"confirmation":"CLEANUP DATA"}),
        ),
    )
    .await;
    assert_eq!(compacted["ok"], true);
    assert!(compacted["data"]["snapshot_backup"].as_str().is_some());
    let settings_bytes = std::fs::read(dir.join("settings.json")).unwrap();
    assert!(!String::from_utf8_lossy(&settings_bytes).contains("\n  "));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn pwa_assets_respect_basic_auth_and_service_worker_cache_rules() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let unauthenticated = Request::builder()
        .uri("/manifest.webmanifest")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        status(&app, unauthenticated).await,
        StatusCode::UNAUTHORIZED
    );

    let manifest_response = app
        .clone()
        .oneshot(auth_get("/manifest.webmanifest"))
        .await
        .unwrap();
    assert_eq!(manifest_response.status(), StatusCode::OK);
    assert!(manifest_response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("manifest") || value.contains("json")));

    let worker_response = app
        .clone()
        .oneshot(auth_get("/service-worker.js"))
        .await
        .unwrap();
    assert_eq!(worker_response.status(), StatusCode::OK);
    assert_eq!(
        worker_response
            .headers()
            .get(header::CACHE_CONTROL)
            .unwrap(),
        "no-cache"
    );
    assert_eq!(
        worker_response
            .headers()
            .get("service-worker-allowed")
            .unwrap(),
        "/"
    );
    assert!(worker_response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("javascript")));

    assert_eq!(
        status(&app, auth_get("/icons/icon-192.png")).await,
        StatusCode::OK
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn p10_openapi_browser_push_and_subscription_tags_are_exposed() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);
    let openapi = json_body(&app, auth_get("/openapi.json")).await;
    assert_eq!(openapi["openapi"], "3.1.0");
    assert!(openapi["paths"]["/api/push/browser"].is_object());
    assert!(openapi["paths"]["/metrics"].is_object());
    assert!(openapi["paths"]["/api/observability/log-filter"].is_object());
    assert!(openapi["paths"]["/api/backups/verification"].is_object());
    assert!(openapi["paths"]["/api/storage/cleanup"].is_object());
    assert!(openapi["paths"]["/api/storage/decision"].is_object());

    let browser = json_body(&app, auth_get("/api/push/browser")).await;
    assert_eq!(browser["ok"], true);
    assert!(browser["data"]["public_key"]
        .as_str()
        .is_some_and(|key| key.len() > 80));

    let created = json_body(
        &app,
        auth_post(
            "/api/subscriptions",
            serde_json::json!({
                "title":"Tagged", "url":"https://pan.quark.cn/s/tagged",
                "media_type":"series", "season":1,
                "tags":["追更", " 4K ", "追更"]
            }),
        ),
    )
    .await;
    assert_eq!(created["data"]["tags"], serde_json::json!(["追更", "4K"]));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn submitted_job_keeps_request_and_correlation_context() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/subscriptions/metadata/scrape")
        .header(
            header::AUTHORIZATION,
            basic_auth_header("admin", "test-secret-pw"),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-request-id", "request-job-1")
        .header("x-correlation-id", "correlation-job-1")
        .body(Body::from(r#"{"overwrite":false}"#))
        .unwrap();

    let (status, _, body) = json_response(&app, request).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["data"]["request_id"], "request-job-1");
    assert_eq!(body["data"]["correlation_id"], "correlation-job-1");
    assert!(body["data"]["id"].as_str().is_some());

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn prometheus_and_runtime_log_filter_are_available() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let response = app.clone().oneshot(auth_get("/metrics")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("text/plain"));
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(body.contains("my_media_sub_subscription_checks_total"));
    assert!(body.contains("my_media_sub_http_requests_total"));

    let updated = json_body(
        &app,
        auth_put(
            "/api/observability/log-filter",
            serde_json::json!({"filter":"info,my_media_sub=debug"}),
        ),
    )
    .await;
    assert_eq!(updated["ok"], true);
    assert!(updated["data"]["filter"]
        .as_str()
        .unwrap()
        .contains("my_media_sub=debug"));

    let invalid = json_response(
        &app,
        auth_put(
            "/api/observability/log-filter",
            serde_json::json!({"filter":"["}),
        ),
    )
    .await;
    assert_eq!(invalid.0, StatusCode::BAD_REQUEST);
    assert_eq!(invalid.2["error"], "validation_error");

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn automation_token_scopes_rotate_and_revoke() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);
    let rotated = json_body(
        &app,
        auth_post(
            "/api/automation-token",
            serde_json::json!({"scopes":["subscriptions:read"],"expires_days":30}),
        ),
    )
    .await;
    let token = rotated["data"]["token"].as_str().unwrap().to_string();
    assert!(token.starts_with("mms_"));
    let bearer = |uri: &str| {
        Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        status(&app, bearer("/api/subscriptions")).await,
        StatusCode::OK
    );
    assert_eq!(
        status(&app, bearer("/api/diagnostics")).await,
        StatusCode::UNAUTHORIZED
    );
    let revoked = json_body(&app, auth_delete("/api/automation-token")).await;
    assert!(revoked["data"]["revoked_at"].as_i64().is_some());
    assert_eq!(
        status(&app, bearer("/api/subscriptions")).await,
        StatusCode::UNAUTHORIZED
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn subscription_export_import_preview_and_idempotency_are_stable() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);
    let created = json_body(
        &app,
        auth_post(
            "/api/subscriptions",
            serde_json::json!({"title":"Exchange","url":"https://pan.quark.cn/s/exchange","media_type":"series","season":1}),
        ),
    )
    .await;
    assert_eq!(created["ok"], true);
    let exported = json_body(&app, auth_get("/api/subscriptions/export")).await;
    assert_eq!(exported["data"]["format"], "my-media-sub-subscriptions");
    let request = serde_json::json!({
        "archive": exported["data"].clone(),
        "strategy": "new_id",
        "confirmation": "IMPORT SUBSCRIPTIONS"
    });
    let preview = json_body(
        &app,
        auth_post("/api/subscriptions/import/preview", request.clone()),
    )
    .await;
    assert_eq!(preview["data"]["conflicts"], 1);
    let import_request = || {
        Request::builder()
            .method(Method::POST)
            .uri("/api/subscriptions/import")
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .header("idempotency-key", "exchange-import-1")
            .body(Body::from(request.to_string()))
            .unwrap()
    };
    let first = json_response(&app, import_request()).await;
    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(first.2["data"]["created"], 1);
    let repeated = json_response(&app, import_request()).await;
    assert_eq!(repeated.0, StatusCode::OK);
    assert_eq!(repeated.2["data"], first.2["data"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn concurrent_subscription_import_with_same_key_runs_once() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);
    let created = json_body(
        &app,
        auth_post(
            "/api/subscriptions",
            serde_json::json!({"title":"Concurrent Exchange","url":"https://pan.quark.cn/s/concurrent-exchange","media_type":"series","season":1}),
        ),
    )
    .await;
    assert_eq!(created["ok"], true);
    let exported = json_body(&app, auth_get("/api/subscriptions/export")).await;
    let body = serde_json::json!({
        "archive": exported["data"].clone(),
        "strategy": "new_id",
        "confirmation": "IMPORT SUBSCRIPTIONS"
    });
    let key = format!("concurrent-import-{}", uuid::Uuid::new_v4());
    let request = || {
        Request::builder()
            .method(Method::POST)
            .uri("/api/subscriptions/import")
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .header("idempotency-key", &key)
            .body(Body::from(body.to_string()))
            .unwrap()
    };

    let (first, second, third, fourth) = tokio::join!(
        json_response(&app, request()),
        json_response(&app, request()),
        json_response(&app, request()),
        json_response(&app, request()),
    );
    let responses = [first, second, third, fourth];
    assert_eq!(
        responses
            .iter()
            .filter(|response| response.0 == StatusCode::CREATED)
            .count(),
        1
    );
    assert!(responses.iter().all(|response| {
        matches!(response.0, StatusCode::CREATED | StatusCode::OK)
            && response.2["data"] == responses[0].2["data"]
    }));

    let subscriptions = json_body(&app, auth_get("/api/subscriptions")).await;
    assert_eq!(subscriptions["data"].as_array().unwrap().len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn telegram_webhook_requires_both_path_and_header_secrets_without_basic_auth() {
    let (ctx, dir) = test_context().await;
    ctx.settings_store
        .update(|settings| {
            settings.telegram_bot_mode = "webhook".to_string();
            settings.telegram_bot_webhook_path_secret = "path-secret-0123456789abcdef".to_string();
            settings.telegram_bot_webhook_secret = "header-secret-0123456789abcdef".to_string();
        })
        .await
        .unwrap();
    ctx.telegram_bot_store
        .add_audit(my_media_sub::store::TelegramCommandAudit {
            id: "telegram-audit-1".to_string(),
            update_id: 1,
            callback_id: None,
            user_id: 42,
            chat_id: 42,
            command: "status".to_string(),
            target: String::new(),
            result: "succeeded".to_string(),
            message: "ok".to_string(),
            duration_ms: 1,
            correlation_id: "telegram-correlation-1".to_string(),
            created_at: 1,
        })
        .await
        .unwrap();
    let app = create_app(ctx.clone());
    let request = |path: &str, secret: Option<&str>| {
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/telegram/webhook/{path}"))
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(secret) = secret {
            builder = builder.header("x-telegram-bot-api-secret-token", secret);
        }
        builder.body(Body::from(r#"{"update_id":123}"#)).unwrap()
    };

    assert_eq!(
        status(
            &app,
            request(
                "wrong-path-secret-0123456789",
                Some("header-secret-0123456789abcdef")
            )
        )
        .await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status(
            &app,
            request(
                "path-secret-0123456789abcdef",
                Some("wrong-header-secret-0123456789")
            )
        )
        .await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status(
            &app,
            request(
                "path-secret-0123456789abcdef",
                Some("header-secret-0123456789abcdef")
            )
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        status(
            &app,
            request(
                "path-secret-0123456789abcdef",
                Some("header-secret-0123456789abcdef")
            )
        )
        .await,
        StatusCode::OK
    );
    let cross_site_webhook = Request::builder()
        .method(Method::POST)
        .uri("/api/telegram/webhook/path-secret-0123456789abcdef")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, "https://telegram.example")
        .header(
            "x-telegram-bot-api-secret-token",
            "header-secret-0123456789abcdef",
        )
        .body(Body::from(r#"{"update_id":124}"#))
        .unwrap();
    assert_eq!(status(&app, cross_site_webhook).await, StatusCode::OK);
    // Webhook handling claims duplicate updates in a spawned task. Poll the
    // observable counter with a bounded deadline instead of racing the task
    // on shared GitHub runners.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        let diagnostics = json_body(&app, auth_get("/api/diagnostics")).await;
        if diagnostics["data"]["telegram_bot"]["deduplicated_updates"] == 1
            || tokio::time::Instant::now() >= deadline
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    let public_settings = json_body(&app, auth_get("/api/settings")).await;
    assert_ne!(
        public_settings["data"]["telegram_bot_webhook_path_secret"],
        "path-secret-0123456789abcdef"
    );
    assert_ne!(
        public_settings["data"]["telegram_bot_webhook_secret"],
        "header-secret-0123456789abcdef"
    );
    assert_eq!(
        public_settings["data"]["telegram_bot_webhook_secret_configured"],
        true
    );

    let diagnostics = json_body(&app, auth_get("/api/diagnostics")).await;
    assert_eq!(diagnostics["data"]["telegram_bot"]["mode"], "disabled");
    assert_eq!(
        diagnostics["data"]["telegram_bot"]["deduplicated_updates"],
        1
    );
    assert_eq!(diagnostics["data"]["telegram_bot"]["audit_count"], 1);
    let audits = json_body(&app, auth_get("/api/telegram/audits")).await;
    assert_eq!(audits["data"][0]["command"], "status");
    assert_eq!(
        audits["data"][0]["correlation_id"],
        "telegram-correlation-1"
    );
    assert!(diagnostics["data"]["telegram_bot"]
        .get("last_error")
        .is_some());

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn quark_signin_automation_scope_is_minimal_and_bot_compatible() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);
    let rotated = json_body(
        &app,
        auth_post(
            "/api/automation-token",
            serde_json::json!({"scopes":["quark:signin"],"expires_days":1}),
        ),
    )
    .await;
    let token = rotated["data"]["token"].as_str().unwrap();
    let signin = Request::builder()
        .method(Method::POST)
        .uri("/api/quark/signin")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    // Scope authentication succeeds; the missing test Cookie is rejected by the existing service.
    assert_eq!(status(&app, signin).await, StatusCode::BAD_REQUEST);
    let subscriptions = Request::builder()
        .uri("/api/subscriptions")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(status(&app, subscriptions).await, StatusCode::UNAUTHORIZED);
    let _ = std::fs::remove_dir_all(dir);
}

// ─── /api/update/* ─────────────────────────────────────────────────────────
// 在线更新是全仓唯一会替换自身可执行文件并重启进程的路径，此前零集成测试。
// 这里只覆盖不触发网络的部分：鉴权、Token scope、进度快照与重启守卫。
// check/releases 的 handler 会真的打 GitHub API，故只验证它们的鉴权拒绝；
// 运行时门控矩阵由 api/update.rs 的 online_update_supported_for 单测覆盖。

#[tokio::test]
async fn update_endpoints_reject_anonymous_access() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    // 注意：认证失败连续 5 次后同一来源会被限流为 429，所以这里恰好用满 5 次，
    // 不能再在本测试里追加需要鉴权成功的请求。
    for uri in [
        "/api/update/check",
        "/api/update/releases",
        "/api/update/progress",
    ] {
        let anonymous = Request::builder().uri(uri).body(Body::empty()).unwrap();
        assert_eq!(
            status(&app, anonymous).await,
            StatusCode::UNAUTHORIZED,
            "{uri} 必须拒绝匿名访问"
        );
    }
    for uri in ["/api/update/apply", "/api/update/restart"] {
        let anonymous = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap();
        assert_eq!(
            status(&app, anonymous).await,
            StatusCode::UNAUTHORIZED,
            "{uri} 必须拒绝匿名访问"
        );
    }

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn update_endpoints_are_closed_to_automation_tokens() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    // 先取 Token，再打升级路径：反过来会先耗尽认证失败额度而被 429 挡住。
    let rotated = json_body(
        &app,
        auth_post(
            "/api/automation-token",
            serde_json::json!({
                "scopes": ["subscriptions:read", "diagnostics:read", "jobs:write"],
                "expires_days": 30
            }),
        ),
    )
    .await;
    let token = rotated["data"]["token"]
        .as_str()
        .expect("轮换自动化 Token 应该成功")
        .to_string();

    // 路径白名单默认拒绝：升级路径不得对任何 scope 开放，否则一个只读 Token
    // 就能触发自替换二进制并重启进程。
    for (method, uri) in [
        (Method::GET, "/api/update/check"),
        (Method::GET, "/api/update/progress"),
        (Method::POST, "/api/update/apply"),
        (Method::POST, "/api/update/restart"),
    ] {
        let bearer = Request::builder()
            .method(method.clone())
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap();
        assert_eq!(
            status(&app, bearer).await,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} 不得对自动化 Token 开放"
        );
    }

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn update_progress_reports_idle_snapshot_in_the_standard_envelope() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let (status_code, headers, body) = json_response(&app, auth_get("/api/update/progress")).await;
    assert_eq!(status_code, StatusCode::OK);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["running"], false);
    assert_eq!(body["data"]["stage"], "idle");
    assert_eq!(body["data"]["percent"], 0);
    assert_eq!(body["data"]["downloaded_bytes"], 0);
    assert!(body["data"]["total_bytes"].is_null());
    assert!(body["data"]["error"].is_null());
    assert!(body["data"]["updated_at"].as_str().is_some());

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn update_restart_without_a_pending_plan_is_rejected() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    // 没有走完 apply 就调 restart，必须是校验错误而不是真的重启进程。
    let (status_code, headers, body) = json_response(
        &app,
        auth_post("/api/update/restart", serde_json::json!({})),
    )
    .await;
    assert_eq!(status_code, StatusCode::BAD_REQUEST);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "validation_error");
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("没有待重启的升级任务"));

    let _ = std::fs::remove_dir_all(dir);
}

// ─── TMDB 测试 ────────────────────────────────────────────────────────────

#[tokio::test]
async fn metadata_test_reports_missing_tmdb_key_without_network() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let (status_code, headers, body) = json_response(&app, auth_get("/api/metadata/test")).await;
    assert_eq!(status_code, StatusCode::OK);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["success"], false);
    assert!(body["data"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("未配置 TMDB API Key"));

    let _ = std::fs::remove_dir_all(dir);
}

// ─── 季度探测（订阅编辑器勾选季度）────────────────────────────────────────

#[tokio::test]
async fn seasons_detection_reports_detected_seasons_from_share() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx.clone());

    // mock fixture：带 Season 路径与 Sxx 文件名的分享内容
    let fixture = std::env::temp_dir().join(format!(
        "my-media-sub-seasons-fixture-{}.json",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(
        &fixture,
        serde_json::json!({
            "https://pan.quark.cn/s/seasons-detect": {
                "ok": true,
                "state": "ok",
                "message": "",
                "files": [
                    {"name": "Show S01E01.mkv", "is_dir": false, "parent_path": "Season 1", "size": 1, "file_key": "f1"},
                    {"name": "Show S01E02.mkv", "is_dir": false, "parent_path": "Season 1", "size": 1, "file_key": "f2"},
                    {"name": "Show.S02E01.mkv", "is_dir": false, "parent_path": "", "size": 1, "file_key": "f3"},
                    {"name": "海报.jpg", "is_dir": false, "parent_path": "Season 1", "size": 1, "file_key": "f4"}
                ]
            }
        })
        .to_string(),
    )
    .unwrap();
    let _fixture_guard = fixture_path_guard().await;
    std::env::set_var("MOCK_QUARK_SHARE_FIXTURE", &fixture);
    ctx.settings_store
        .update(|settings| settings.quark_cookie = "test-cookie".to_string())
        .await
        .unwrap();

    let (status, headers, body) = json_response(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/subscriptions/seasons")
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "url": "https://pan.quark.cn/s/seasons-detect",
                    "password": ""
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], serde_json::json!(true));
    let seasons = body["data"]["seasons"].as_array().unwrap();
    let detected: Vec<i64> = seasons
        .iter()
        .map(|season| season["season"].as_i64().unwrap())
        .collect();
    assert_eq!(detected, vec![1, 2]);
    assert_eq!(seasons[0]["file_count"], serde_json::json!(2));
    assert_eq!(body["data"]["total_file_count"], serde_json::json!(3));
    assert_eq!(body["data"]["unspecified_file_count"], serde_json::json!(0));

    std::env::remove_var("MOCK_QUARK_SHARE_FIXTURE");
    let _ = std::fs::remove_file(&fixture);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn seasons_detection_rejects_missing_url_and_empty_cookie() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx.clone());

    // 未配置 Cookie：直接拒绝并给出可操作提示
    let (status, _, body) = json_response(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/subscriptions/seasons")
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({"url": "https://pan.quark.cn/s/x"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"].as_str().unwrap().contains("Cookie"));

    ctx.settings_store
        .update(|settings| settings.quark_cookie = "test-cookie".to_string())
        .await
        .unwrap();
    // 空 URL：参数校验失败
    let (status, _, _) = json_response(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/subscriptions/seasons")
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::json!({"url": " "}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn seasons_detection_infers_season_one_for_unmarked_share() {
    // 很多资源不标季度，文件名就是 01/02。端到端确认这类分享被按第一季
    // 处理并标记为 inferred，用户无需再手填季号。
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx.clone());

    let fixture = std::env::temp_dir().join(format!(
        "my-media-sub-seasons-plain-fixture-{}.json",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(
        &fixture,
        serde_json::json!({
            "https://pan.quark.cn/s/seasons-plain": {
                "ok": true,
                "state": "ok",
                "message": "",
                "files": [
                    {"name": "01.mkv", "is_dir": false, "parent_path": "", "size": 1, "file_key": "f1"},
                    {"name": "02.mkv", "is_dir": false, "parent_path": "", "size": 1, "file_key": "f2"},
                    {"name": "03.mkv", "is_dir": false, "parent_path": "", "size": 1, "file_key": "f3"}
                ]
            }
        })
        .to_string(),
    )
    .unwrap();
    let _fixture_guard = fixture_path_guard().await;
    std::env::set_var("MOCK_QUARK_SHARE_FIXTURE", &fixture);
    ctx.settings_store
        .update(|settings| settings.quark_cookie = "test-cookie".to_string())
        .await
        .unwrap();

    let (status, headers, body) = json_response(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/subscriptions/seasons")
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "url": "https://pan.quark.cn/s/seasons-plain",
                    "password": ""
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_json_content_type(&headers);
    assert_eq!(body["ok"], serde_json::json!(true));
    let seasons = body["data"]["seasons"].as_array().unwrap();
    assert_eq!(seasons.len(), 1);
    assert_eq!(seasons[0]["season"], serde_json::json!(1));
    assert_eq!(seasons[0]["file_count"], serde_json::json!(3));
    assert_eq!(seasons[0]["inferred"], serde_json::json!(true));
    assert_eq!(body["data"]["unspecified_file_count"], serde_json::json!(0));
    assert_eq!(body["data"]["total_file_count"], serde_json::json!(3));
    assert!(body["data"]["message"]
        .as_str()
        .unwrap()
        .contains("按第一季处理"));

    std::env::remove_var("MOCK_QUARK_SHARE_FIXTURE");
    let _ = std::fs::remove_file(&fixture);
    let _ = std::fs::remove_dir_all(dir);
}

/// 串行化对 MOCK_QUARK_SHARE_FIXTURE 环境变量的读写，避免并行测试互相覆盖。
/// guard 需要跨 await 持有（覆盖 oneshot 请求期间），因此使用异步锁。
async fn fixture_path_guard() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    LOCK.lock().await
}

#[tokio::test]
async fn subscription_create_roundtrips_skip_season_list() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx.clone());

    // 创建跳季订阅（只订 S1+S3）
    let (status, _, body) = json_response(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/subscriptions")
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "title": "跳季测试剧",
                    "url": "https://pan.quark.cn/s/skip-season",
                    "media_type": "series",
                    "season_list": [1, 3]
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["data"]["season"], serde_json::json!(1));
    assert_eq!(body["data"]["season_end"], serde_json::json!(3));
    assert_eq!(
        body["data"]["season_list"],
        serde_json::json!([1, 3]),
        "跳季集合应原样保留"
    );

    // 连续列表折叠为区间语义
    let (status, _, body) = json_response(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/subscriptions")
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "title": "连续季测试剧",
                    "url": "https://pan.quark.cn/s/contiguous-season",
                    "media_type": "series",
                    "season_list": [1, 2, 3]
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["data"]["season_list"], serde_json::Value::Null);
    assert_eq!(body["data"]["season"], serde_json::json!(1));
    assert_eq!(body["data"]["season_end"], serde_json::json!(3));

    // 逗号 season_spec 等价于 season_list
    let (status, _, body) = json_response(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/subscriptions")
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "title": "逗号语法测试剧",
                    "url": "https://pan.quark.cn/s/comma-spec",
                    "media_type": "series",
                    "season_spec": "1,3"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["data"]["season_list"], serde_json::json!([1, 3]));

    // 更新：空数组清除集合回到区间语义
    let sub_id = ctx
        .subscription_store
        .list()
        .await
        .into_iter()
        .find(|sub| sub.url == "https://pan.quark.cn/s/skip-season")
        .map(|sub| sub.id)
        .unwrap();
    let (status, _, body) = json_response(
        &app,
        Request::builder()
            .method(Method::PUT)
            .uri(format!("/api/subscriptions/{sub_id}"))
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({"season_list": []}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["season_list"], serde_json::Value::Null);
    assert_eq!(body["data"]["season"], serde_json::json!(1));
    assert_eq!(body["data"]["season_end"], serde_json::json!(3));

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn parse_season_endpoint_supports_comma_set() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let (status, _, body) = json_response(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/utils/parse-season")
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({"season_spec": "1,3"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["season"], serde_json::json!(1));
    assert_eq!(body["data"]["season_end"], serde_json::json!(3));
    assert_eq!(body["data"]["season_list"], serde_json::json!([1, 3]));
    assert_eq!(body["data"]["seasons"], serde_json::json!([1, 3]));
    assert_eq!(body["data"]["season_spec"], serde_json::json!("1,3"));
    assert_eq!(body["data"]["label"], serde_json::json!("第 1,3 季"));

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn season_range_edit_preserves_transfer_progress() {
    // 回归：编辑订阅增加季度（暂不转存 → 转存该季）不得清空已转存/已知
    // 进度——旧行为会整体重置，扩季后已转存的季被重复转存。
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx.clone());

    let (status, _, body) = json_response(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/subscriptions")
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "title": "扩季测试剧",
                    "url": "https://pan.quark.cn/s/expand-season",
                    "media_type": "series",
                    "season_list": [1, 3]
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let sub_id = body["data"]["id"].as_str().unwrap().to_string();

    // 预置检查进度（模拟已按 [1,3] 检查/转存过 S1、S3）
    ctx.subscription_store
        .update(&sub_id, |sub| {
            sub.known_files = vec!["Show S01E01.mkv".to_string(), "Show S03E01.mkv".to_string()];
            sub.known_file_keys = vec!["f1".to_string(), "f3".to_string()];
            sub.transferred_files =
                vec!["Show S01E01.mkv".to_string(), "Show S03E01.mkv".to_string()];
            sub.transferred_file_keys = vec!["ep:1".to_string(), "ep:1".to_string()];
            sub.completed = true;
            sub.status = "completed".to_string();
        })
        .await
        .unwrap();

    // 编辑订阅：加上 S2（连续集合折叠为区间）
    let (status, _, body) = json_response(
        &app,
        Request::builder()
            .method(Method::PUT)
            .uri(format!("/api/subscriptions/{sub_id}"))
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({"season_list": [1, 2, 3]}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["season_list"], serde_json::Value::Null);
    assert_eq!(body["data"]["season"], serde_json::json!(1));
    assert_eq!(body["data"]["season_end"], serde_json::json!(3));

    // 进度保留：已转存/已知记录原样存在，不因扩季被清空
    let sub = ctx.subscription_store.get(&sub_id).await.unwrap();
    assert_eq!(sub.known_files.len(), 2, "扩季不得清空已知文件");
    assert_eq!(sub.transferred_files.len(), 2, "扩季不得清空已转存记录");
    // 完结状态释放：扩季后订阅回到追更中，可继续检查新季
    assert!(!sub.completed, "扩季应解除完结状态以便发现新季");
    assert_eq!(sub.status, "active");
    assert_eq!(
        sub.total_episode_number, None,
        "单季完结目标应随范围调整清除"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn skip_season_subscription_does_not_backfill_single_season_total() {
    // 回归：跳季/多季订阅的任意一次编辑都不得把 min 季的单季集数写成
    // 完结目标——否则会误判完结并封顶其他季超过该集数的文件。
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx.clone());

    let (status, _, body) = json_response(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/subscriptions")
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "title": "跳季回填测试剧",
                    "url": "https://pan.quark.cn/s/skip-backfill",
                    "media_type": "series",
                    "season_list": [1, 3],
                    "metadata": {
                        "provider": "tmdb",
                        "provider_id": "1",
                        "title": "跳季回填测试剧",
                        "media_type": "series",
                        "number_of_episodes": 12,
                        "number_of_seasons": 3,
                        "seasons": [
                            {"season_number": 1, "episode_count": 12, "name": "Season 1"}
                        ]
                    }
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let sub_id = body["data"]["id"].as_str().unwrap().to_string();

    // 任意一次不带 total_episode_number 的编辑（改名）后：
    // S1 的 12 集不得成为完结目标
    let (status, _, body) = json_response(
        &app,
        Request::builder()
            .method(Method::PUT)
            .uri(format!("/api/subscriptions/{sub_id}"))
            .header(
                header::AUTHORIZATION,
                basic_auth_header("admin", "test-secret-pw"),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({"title": "跳季回填测试剧（改名）"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["data"]["total_episode_number"],
        serde_json::Value::Null,
        "跳季订阅不得回填单季集数作为完结目标"
    );
    assert_eq!(body["data"]["completed"], serde_json::json!(false));
    assert_eq!(body["data"]["season_list"], serde_json::json!([1, 3]));

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn season_edits_keep_historical_evidence_without_completing_the_new_season() {
    let (ctx, dir) = test_context().await;
    let sub = serde_json::from_value(serde_json::json!({
        "id":"season-edit-review", "title":"Review Show", "url":"https://pan.quark.cn/s/review",
        "media_type":"series", "season":1, "total_episode_number":2,
        "known_episodes":[2], "current_episode_number":2,
        "known_files":["02.mkv"], "known_file_keys":["ep:2"],
        "transferred_files":["02.mkv"], "transferred_file_keys":["ep:2"],
        "last_new_files":["02.mkv"], "last_new_episodes":[2],
        "last_probe":{"ok":true,"state":"ok","message":"","files":[
            {"name":"02.mkv","file_key":"file-2","size":1,"is_dir":false}
        ]},
        "completed":true, "status":"completed", "created_at":1, "updated_at":1, "last_checked_at":1,
        "metadata":{"provider":"tmdb", "provider_id":"review", "title":"Review Show", "number_of_seasons":2,
            "seasons":[{"season_number":1,"episode_count":2},{"season_number":2,"episode_count":2}]}
    })).unwrap();
    ctx.subscription_store.create(sub).await.unwrap();
    let app = create_app(ctx.clone());
    let path = "/api/subscriptions/season-edit-review";
    let updated = json_body(&app, auth_put(path, serde_json::json!({"season_spec":"2"}))).await;
    assert_eq!(updated["ok"], true);
    assert_eq!(updated["data"]["completed"], false);
    assert_eq!(updated["data"]["current_episode_number"], 0);
    assert_eq!(updated["data"]["total_episode_number"], 2);
    assert_eq!(
        updated["data"]["transferred_files"],
        serde_json::json!(["Season 1/02.mkv"])
    );
    let detail = json_body(
        &app,
        auth_get("/api/subscriptions/season-edit-review/status"),
    )
    .await;
    assert_eq!(detail["data"]["summary"]["transferred_count"], 0);
    assert_eq!(detail["data"]["summary"]["discovered_count"], 0);
    // Switching back recovers the retained S1 evidence, rather than retransferring it.
    let back = json_body(&app, auth_put(path, serde_json::json!({"season_spec":"1"}))).await;
    assert_eq!(back["data"]["completed"], true);
    let expanded = json_body(
        &app,
        auth_put(path, serde_json::json!({"season_spec":"1-2"})),
    )
    .await;
    assert_eq!(expanded["data"]["completed"], false);
    let narrowed = json_body(&app, auth_put(path, serde_json::json!({"season_spec":"2"}))).await;
    assert_eq!(narrowed["data"]["completed"], false);
    ctx.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn multi_season_details_and_calendar_keep_selected_seasons_separate() {
    let (ctx, dir) = test_context().await;
    let sub = serde_json::from_value(serde_json::json!({
        "id":"multi-review", "title":"Review Show", "url":"https://pan.quark.cn/s/review",
        "media_type":"series", "season":1, "season_end":3, "season_list":[1,3],
        "known_files":["Season 1/01.mkv", "Season 2/01.mkv", "Season 3/01.mkv"],
        "transferred_files":["Season 1/01.mkv"], "transferred_file_keys":["s:1:ep:1"],
        "created_at":1, "updated_at":1, "last_checked_at":1,
        "metadata":{"provider":"tmdb", "provider_id":"review", "title":"Review Show", "number_of_seasons":3,
            "seasons":[{"season_number":1,"episode_count":1},{"season_number":2,"episode_count":1},{"season_number":3,"episode_count":1}],
            "episodes":[
                {"season_number":1,"episode_number":1,"air_date":"2026-09-05"},
                {"season_number":2,"episode_number":1,"air_date":"2026-09-05"},
                {"season_number":3,"episode_number":1,"air_date":"2026-09-05"}
            ]}
    })).unwrap();
    ctx.subscription_store.create(sub).await.unwrap();
    let app = create_app(ctx.clone());
    let detail = json_body(&app, auth_get("/api/subscriptions/multi-review/status")).await;
    assert_eq!(detail["data"]["summary"]["discovered_count"], 2);
    assert_eq!(detail["data"]["summary"]["transferred_count"], 1);
    assert_eq!(detail["data"]["summary"]["pending_transfer_count"], 1);
    let episodes = detail["data"]["episodes"].as_array().unwrap();
    assert_eq!(episodes.len(), 2);
    assert_eq!(episodes[0]["season"], 1);
    assert_eq!(episodes[1]["season"], 3);
    assert_eq!(episodes[1]["transferred"], false);
    let calendar = json_body(
        &app,
        auth_get("/api/calendar?from=2026-09-05&to=2026-09-05&subscription=multi-review"),
    )
    .await;
    let items = calendar["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let third = items.iter().find(|item| item["season"] == 3).unwrap();
    assert_eq!(third["episode"], 1);
    assert_eq!(third["transferred"], false);
    assert_ne!(items[0]["id"], items[1]["id"]);
    ctx.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn backup_restore_is_applied_before_new_stores_load_despite_old_process_writes() {
    let (ctx, dir) = test_context().await;
    let sub = serde_json::from_value(serde_json::json!({
        "id":"restore-review", "title":"Archived Title", "url":"https://pan.quark.cn/s/review",
        "created_at":1,"updated_at":1,"last_checked_at":1
    }))
    .unwrap();
    ctx.subscription_store.create(sub).await.unwrap();
    let archive = ctx.backup_service.export_archive().await.unwrap();
    ctx.subscription_store
        .update("restore-review", |sub| sub.title = "Newer Title".into())
        .await
        .unwrap();
    let app = create_app(ctx.clone());
    let staged = json_body(
        &app,
        auth_post(
            "/api/backups/restore",
            serde_json::json!({
                "archive":archive, "confirmation":"RESTORE DATA"
            }),
        ),
    )
    .await;
    assert_eq!(staged["ok"], true);
    assert_eq!(staged["data"]["restored_files"], 0);
    let response = json_body(
        &app,
        auth_put(
            "/api/subscriptions/restore-review",
            serde_json::json!({"enabled":false}),
        ),
    )
    .await;
    assert_eq!(response["ok"], true);
    ctx.job_queue.shutdown().await;
    drop(app);
    drop(ctx);
    let config = Config {
        server: my_media_sub::config::ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
        data_dir: dir.clone(),
    };
    let restarted = AppContext::new(&config).await.unwrap();
    let restored = restarted
        .subscription_store
        .get("restore-review")
        .await
        .unwrap();
    assert_eq!(restored.title, "Archived Title");
    assert!(restored.enabled);
    assert!(!dir.join("backups/restore-pending.json").exists());
    restarted
        .subscription_store
        .update("restore-review", |sub| sub.enabled = false)
        .await
        .unwrap();
    let on_disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("subscriptions.json")).unwrap()).unwrap();
    assert_eq!(on_disk["data"][0]["title"], "Archived Title");
    restarted.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}
