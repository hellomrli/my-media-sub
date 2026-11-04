use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use base64::{engine::general_purpose, Engine};
use my_media_sub::{
    api::create_app,
    app::AppContext,
    config::{Config, ServerConfig},
};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
use tower::ServiceExt;

async fn context() -> (Arc<AppContext>, Router, PathBuf) {
    let dir = std::env::temp_dir().join(format!("media-save-api-{}", uuid::Uuid::new_v4()));
    let ctx = AppContext::new(&Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    })
    .await
    .unwrap();
    ctx.settings_store
        .update(|settings| {
            settings.app_password = "media-save-test-password".to_string();
            settings.quark_cookie = "test-cookie".to_string();
            settings.aria2_rpc_url.clear();
        })
        .await
        .unwrap();
    (ctx.clone(), create_app(ctx), dir)
}

async fn request(app: &Router, method: Method, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(
                    header::AUTHORIZATION,
                    format!(
                        "Basic {}",
                        general_purpose::STANDARD.encode("admin:media-save-test-password")
                    ),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn share_probe_exposes_folder_qualities_and_exact_movie_versions() {
    let (_ctx, app, dir) = context().await;
    let fixture = dir.join("probe-fixture.json");
    std::fs::write(&fixture, json!({
        "https://pan.quark.cn/s/movie-versions": {
            "ok": true, "state": "partial", "message": "部分目录未读取",
            "files": [
                {"name": "Movie.mkv", "parent_path": "1080P", "size": 5, "file_key": "hd"},
                {"name": "Movie.mkv", "parent_path": "4K", "size": 20, "file_key": "uhd"},
                {"name": "Movie.Directors.Cut.mkv", "parent_path": "4K", "size": 24, "file_key": "cut"},
                {"name": "Movie.trailer.mp4", "parent_path": "4K", "size": 1, "file_key": "trailer"}
            ]
        }
    }).to_string()).unwrap();
    std::env::set_var("MOCK_QUARK_SHARE_FIXTURE", &fixture);
    let (status, response) = request(
        &app,
        Method::POST,
        "/api/subscriptions/seasons",
        json!({"url": "https://pan.quark.cn/s/movie-versions"}),
    )
    .await;
    std::env::remove_var("MOCK_QUARK_SHARE_FIXTURE");
    assert_eq!(status, StatusCode::OK);
    let data = &response["data"];
    assert_eq!(data["qualities"][0]["resolution"], "2160p");
    assert_eq!(data["qualities"][0]["file_count"], 2);
    assert_eq!(data["qualities"][0]["total_size"], 44);
    assert_eq!(data["qualities"][1]["resolution"], "1080p");
    assert_eq!(data["media_files"].as_array().unwrap().len(), 3);
    assert_eq!(data["media_files"][0]["file_key"], "hd");
    assert_eq!(data["partial"], true);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn saving_a_movie_queues_a_selected_job_without_creating_a_subscription() {
    let (ctx, app, dir) = context().await;
    let payload = json!({"url": "https://pan.quark.cn/s/movie", "save_options": {
        "title": "Movie", "media_type": "movie", "preferred_resolution": "4K",
        "file_ids": ["chosen-uhd"], "target_dir": "/Movies/Movie"
    }});
    let (status, response) = request(&app, Method::POST, "/api/transfer", payload.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["data"]["success"], true);
    let job_id = response["data"]["job_id"].as_str().unwrap();
    let job = ctx.job_store.get(job_id).await.unwrap();
    assert_eq!(job.payload["save_options"]["preferred_resolution"], "2160p");
    assert_eq!(
        job.payload["save_options"]["file_ids"],
        json!(["chosen-uhd"])
    );
    assert!(ctx.subscription_store.list().await.is_empty());
    let (_, repeated) = request(&app, Method::POST, "/api/transfer", payload).await;
    assert_eq!(repeated["data"]["job_id"], job_id);
    assert_eq!(ctx.job_store.list().await.len(), 1);
    ctx.job_store
        .update(job_id, |job| {
            job.status = my_media_sub::jobs::JobStatus::Failed;
            job.result = Some(json!({"saved_count": 1}));
        })
        .await
        .unwrap();
    let (status, retry) = request(
        &app,
        Method::POST,
        &format!("/api/jobs/{job_id}/retry"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(retry["message"].as_str().unwrap().contains("不要重新转存"));
    assert_eq!(ctx.job_store.list().await.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn movie_save_rejects_invalid_quality_multiple_versions_and_missing_downloader() {
    let (ctx, app, dir) = context().await;
    for options in [
        json!({"title": "Movie", "media_type": "movie", "preferred_resolution": "nonsense", "file_ids": ["hd"]}),
        json!({"title": "Movie", "media_type": "movie", "file_ids": ["hd", "uhd"]}),
        json!({"title": "Movie", "media_type": "movie", "sync_download_enabled": true, "file_ids": ["hd"]}),
    ] {
        let (status, _) = request(
            &app,
            Method::POST,
            "/api/transfer",
            json!({"url": "https://pan.quark.cn/s/movie", "save_options": options}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    assert!(ctx.job_store.list().await.is_empty());
    assert!(ctx.subscription_store.list().await.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn subscription_quality_is_canonical_persistent_and_backward_compatible() {
    let (ctx, app, dir) = context().await;
    let (status, created) = request(&app, Method::POST, "/api/subscriptions", json!({
        "title": "Show", "url": "https://pan.quark.cn/s/show", "media_type": "series", "rules": {"preferred_resolution": "4K"}
    })).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["data"]["id"].as_str().unwrap();
    assert_eq!(created["data"]["rules"]["preferred_resolution"], "2160p");
    let (status, updated) = request(
        &app,
        Method::PUT,
        &format!("/api/subscriptions/{id}"),
        json!({"rules": {"preferred_resolution": "1080P"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["rules"]["preferred_resolution"], "1080p");
    assert_eq!(
        ctx.subscription_store
            .get(id)
            .await
            .unwrap()
            .rules
            .preferred_resolution,
        "1080p"
    );
    let (_, legacy) = request(
        &app,
        Method::POST,
        "/api/subscriptions",
        json!({"title": "Legacy", "url": "https://pan.quark.cn/s/legacy"}),
    )
    .await;
    assert_eq!(legacy["data"]["rules"]["preferred_resolution"], "");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn updating_quality_from_a_preset_normalizes_aliases_and_rejects_invalid_values() {
    let (ctx, app, dir) = context().await;
    ctx.settings_store
        .update(|settings| {
            settings.rule_presets = serde_json::from_value(json!([
                {"id": "uhd", "rules": {"preferred_resolution": "4K"}},
                {"id": "invalid", "rules": {"preferred_resolution": "nonsense"}}
            ]))
            .unwrap();
        })
        .await
        .unwrap();
    let (_, created) = request(
        &app,
        Method::POST,
        "/api/subscriptions",
        json!({"title": "Show", "url": "https://pan.quark.cn/s/preset-show"}),
    )
    .await;
    let id = created["data"]["id"].as_str().unwrap();
    let path = format!("/api/subscriptions/{id}");
    let (status, updated) =
        request(&app, Method::PUT, &path, json!({"rule_preset_id": "uhd"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["rules"]["preferred_resolution"], "2160p");
    let (status, _) = request(
        &app,
        Method::PUT,
        &path,
        json!({"rule_preset_id": "invalid"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        ctx.subscription_store
            .get(id)
            .await
            .unwrap()
            .rules
            .preferred_resolution,
        "2160p"
    );
    let (status, updated) = request(
        &app,
        Method::PUT,
        &path,
        json!({"rule_preset_id": "invalid", "rules": {"preferred_resolution": "1080P"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["rules"]["preferred_resolution"], "1080p");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn legacy_movie_records_no_longer_participate_in_periodic_tracking() {
    let (ctx, app, dir) = context().await;
    let (_, created) = request(
        &app,
        Method::POST,
        "/api/subscriptions",
        json!({"title": "Legacy movie", "media_type": "movie", "url": "not-a-share"}),
    )
    .await;
    let id = created["data"]["id"].as_str().unwrap();
    ctx.subscription_store
        .update(id, |sub| sub.last_checked_at = 0)
        .await
        .unwrap();
    let checked = ctx
        .check_service
        .check_due_subscriptions("test-cookie")
        .await
        .unwrap();
    assert!(checked.is_empty());
    assert_eq!(
        ctx.subscription_store
            .get(id)
            .await
            .unwrap()
            .last_checked_at,
        0
    );
    let _ = std::fs::remove_dir_all(dir);
}
