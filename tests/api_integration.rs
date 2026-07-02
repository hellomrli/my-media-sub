/// HTTP 层集成测试：通过 axum 的 oneshot 机制直接在进程内发起请求，
/// 不启动真实 TCP 侦听器，快速验证路由、鉴权和 CRUD 行为。
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use base64::{engine::general_purpose, Engine};
use my_media_sub::{api::create_app, app::AppContext, config::Config};
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

// ─── 测试辅助函数 ───────────────────────────────────────────────────────────

/// 创建使用临时目录的 AppContext（不启动后台调度器）
async fn test_context() -> (Arc<AppContext>, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "my-media-sub-api-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let config = Config {
        server: my_media_sub::config::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            username: "admin".to_string(),
            password: "change-me".to_string(),
        },
        data_dir: dir.clone(),
    };

    let context = AppContext::new(&config).await.expect("test context init failed");
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
async fn json_body(
    app: &axum::Router,
    req: Request<Body>,
) -> serde_json::Value {
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
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

    assert_eq!(status(&app, req).await, StatusCode::OK);
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

    assert_eq!(status(&app, req).await, StatusCode::UNAUTHORIZED);
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

    assert_eq!(status(&app, req).await, StatusCode::UNAUTHORIZED);
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
            basic_auth_header("admin", "change-me"),
        )
        .body(Body::empty())
        .unwrap();

    assert_eq!(status(&app, req).await, StatusCode::OK);
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
            basic_auth_header("admin", "change-me"),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();

    assert_eq!(status(&app, req).await, StatusCode::FORBIDDEN);
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
            basic_auth_header("admin", "change-me"),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();

    assert_eq!(status(&app, req).await, StatusCode::FORBIDDEN);
    let _ = std::fs::remove_dir_all(dir);
}

// ─── 订阅 CRUD ────────────────────────────────────────────────────────────

fn auth_get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(
            header::AUTHORIZATION,
            basic_auth_header("admin", "change-me"),
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
            basic_auth_header("admin", "change-me"),
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
            basic_auth_header("admin", "change-me"),
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
    let id = created["data"]["id"].as_str().expect("created sub should have id");
    assert!(!id.is_empty());

    // 用 GET 能取回
    let list = json_body(&app, auth_get("/api/subscriptions")).await;
    let items = list["data"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str().unwrap(), id);

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

    let s2 = app
        .clone()
        .oneshot(auth_post("/api/subscriptions", payload))
        .await
        .unwrap()
        .status();
    assert_eq!(s2, StatusCode::BAD_REQUEST);

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

    let del_status = status(&app, auth_delete(&format!("/api/subscriptions/{id}"))).await;
    assert_eq!(del_status, StatusCode::NO_CONTENT);

    // 再 GET 确认已消失
    let list = json_body(&app, auth_get("/api/subscriptions")).await;
    assert!(list["data"].as_array().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn get_nonexistent_subscription_returns_404() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let s = status(&app, auth_get("/api/subscriptions/no-such-id")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

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

    let _ = std::fs::remove_dir_all(dir);
}
