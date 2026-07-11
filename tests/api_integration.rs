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
            username: "admin".to_string(),
            password: "change-me".to_string(),
        },
        data_dir: dir.clone(),
    };

    let context = AppContext::new(&config)
        .await
        .expect("test context init failed");
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

#[tokio::test]
async fn strm_errors_remain_non_enveloped_media_responses() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let req = Request::builder()
        .uri("/strm/quark/test-fid/test.mkv")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/plain")));

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
            basic_auth_header("admin", "change-me"),
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
            basic_auth_header("admin", "change-me"),
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
            basic_auth_header("admin", "change-me"),
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
            basic_auth_header("admin", "change-me"),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{"))
        .unwrap();
    let (status, headers, body) = json_response(&app, req).await;

    assert!(matches!(
        status,
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
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
            basic_auth_header("admin", "change-me"),
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

fn auth_put(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(Method::PUT)
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
    assert_eq!(
        body["data"]["missing_episodes"],
        serde_json::json!([3, 5, 6])
    );
    assert_eq!(body["data"]["episodes"].as_array().unwrap().len(), 6);
    assert_eq!(body["data"]["pipeline"].as_array().unwrap().len(), 7);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn calendar_returns_manual_schedule_with_summary_and_actions() {
    let (ctx, dir) = test_context().await;
    let app = create_app(ctx);

    let payload = serde_json::json!({
        "title": "Calendar Test Series",
        "url": "https://pan.quark.cn/s/calendar-test",
        "media_type": "series",
        "season": 1,
        "manual_schedule": {
            "start_date": "2026-07-06",
            "weekdays": [1, 4],
            "air_time": "20:30",
            "interval_weeks": 1,
            "first_episode_number": 1,
            "total_episodes": 4
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
    let subscription_id = created["data"]["id"].as_str().unwrap();

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
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items[0]["scheduled_at"], "2026-07-06T20:30:00+08:00");
    assert_eq!(items[0]["schedule_source"], "manual");
    assert_eq!(items[0]["actions"]["can_check"], true);
    assert!(items[0]["actions"]["detail_url"]
        .as_str()
        .unwrap()
        .contains("subscription="));

    let clear_response = app
        .clone()
        .oneshot(auth_put(
            &format!("/api/subscriptions/{subscription_id}"),
            serde_json::json!({"manual_schedule": null}),
        ))
        .await
        .unwrap();
    assert_eq!(clear_response.status(), StatusCode::OK);
    let cleared = json_body(
        &app,
        auth_get(&format!("/api/subscriptions/{subscription_id}")),
    )
    .await;
    assert!(cleared["data"].get("manual_schedule").is_none());

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn calendar_rejects_invalid_query_and_manual_schedule() {
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

    let invalid_payload = serde_json::json!({
        "title": "Invalid Calendar Series",
        "url": "https://pan.quark.cn/s/calendar-invalid",
        "media_type": "series",
        "manual_schedule": {
            "start_date": "2026/07/06",
            "weekdays": [8],
            "air_time": "25:00"
        }
    });
    let (status, headers, body) =
        json_response(&app, auth_post("/api/subscriptions", invalid_payload)).await;
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
                    basic_auth_header("admin", "change-me"),
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

    let preview = json_body(&app, auth_post("/api/backups/preview", archive)).await;
    assert_eq!(preview["ok"], true);
    assert_eq!(preview["data"]["valid"], true);
    assert!(preview["data"]["file_count"].as_u64().unwrap() >= 1);

    let diagnostics = json_body(&app, auth_get("/api/diagnostics")).await;
    assert_eq!(diagnostics["ok"], true);
    assert_eq!(diagnostics["data"]["schema_version"], 1);
    assert_eq!(
        diagnostics["data"]["storage_decision"]["recommendation"],
        "keep_json"
    );
    assert!(diagnostics["data"]["metrics"]["store_io"].is_object());
    assert!(diagnostics.to_string().find("change-me").is_none());

    let compacted = json_body(
        &app,
        auth_post(
            "/api/storage/compact",
            serde_json::json!({"confirmation":"COMPACT JSON"}),
        ),
    )
    .await;
    assert_eq!(compacted["ok"], true);
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
