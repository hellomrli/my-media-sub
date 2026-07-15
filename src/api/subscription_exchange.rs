use super::response::ApiResponse;
use crate::{
    error::{AppError, Result},
    models::Subscription,
    store::SubscriptionStore,
    utils::unix_now,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};
use tokio::sync::Mutex;

#[derive(Clone, Serialize, Deserialize)]
struct Archive {
    format: String,
    version: u32,
    exported_at: i64,
    subscriptions: Vec<Subscription>,
}
#[derive(Deserialize)]
struct ImportRequest {
    archive: Archive,
    strategy: String,
    confirmation: String,
}
#[derive(Clone, Serialize)]
struct ImportPreview {
    total: usize,
    conflicts: usize,
    creates: usize,
    strategy: String,
}
#[derive(Clone, Serialize)]
struct ImportResult {
    created: usize,
    updated: usize,
    skipped: usize,
}
#[derive(Clone)]
struct Idempotent {
    fingerprint: String,
    result: ImportResult,
    created_at: i64,
}
static IDEMPOTENCY: LazyLock<Mutex<HashMap<String, Idempotent>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

async fn export(State(store): State<Arc<SubscriptionStore>>) -> Json<ApiResponse<Archive>> {
    Json(ApiResponse::ok(Archive {
        format: "my-media-sub-subscriptions".into(),
        version: 1,
        exported_at: unix_now(),
        subscriptions: store.list().await,
    }))
}
fn validate(req: &ImportRequest) -> Result<()> {
    if req.archive.format != "my-media-sub-subscriptions" || req.archive.version != 1 {
        Err(AppError::Validation("不支持的订阅导入格式".into()))
    } else if !matches!(req.strategy.as_str(), "skip" | "update" | "new_id") {
        Err(AppError::Validation("导入策略无效".into()))
    } else {
        Ok(())
    }
}
async fn preview(
    State(store): State<Arc<SubscriptionStore>>,
    Json(req): Json<ImportRequest>,
) -> Result<Json<ApiResponse<ImportPreview>>> {
    validate(&req)?;
    let current = store.list().await;
    let conflicts = req
        .archive
        .subscriptions
        .iter()
        .filter(|item| {
            current
                .iter()
                .any(|c| c.id == item.id || c.url == item.url && c.title == item.title)
        })
        .count();
    Ok(Json(ApiResponse::ok(ImportPreview {
        total: req.archive.subscriptions.len(),
        conflicts,
        creates: req.archive.subscriptions.len() - conflicts,
        strategy: req.strategy,
    })))
}
async fn import(
    State(store): State<Arc<SubscriptionStore>>,
    headers: HeaderMap,
    Json(req): Json<ImportRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ImportResult>>)> {
    validate(&req)?;
    if req.confirmation != "IMPORT SUBSCRIPTIONS" {
        return Err(AppError::Validation(
            "导入确认文本必须为 IMPORT SUBSCRIPTIONS".into(),
        ));
    }
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty() && v.len() <= 128)
        .ok_or_else(|| AppError::Validation("订阅导入必须提供 Idempotency-Key".into()))?
        .to_string();
    let body = serde_json::to_vec(&req.archive)?;
    let fingerprint = format!(
        "{:x}",
        md5::compute([body, req.strategy.as_bytes().to_vec()].concat())
    );
    let mut records = IDEMPOTENCY.lock().await;
    records.retain(|_, r| unix_now() - r.created_at < 86400);
    if let Some(old) = records.get(&key) {
        if old.fingerprint != fingerprint {
            return Err(AppError::Validation(
                "Idempotency-Key 已用于不同请求".into(),
            ));
        }
        return Ok((StatusCode::OK, Json(ApiResponse::ok(old.result.clone()))));
    }

    // 保持幂等锁直到导入结果已写入记录，消除两个同 Key 请求都通过检查的
    // TOCTOU 窗口。导入只涉及本地原子 Store 写入，临界区是有界的。
    let (created, updated, skipped) = store
        .import_batch(req.archive.subscriptions, &req.strategy)
        .await?;
    let result = ImportResult {
        created,
        updated,
        skipped,
    };
    records.insert(
        key,
        Idempotent {
            fingerprint,
            result: result.clone(),
            created_at: unix_now(),
        },
    );
    Ok((StatusCode::CREATED, Json(ApiResponse::ok(result))))
}
pub fn routes(store: Arc<SubscriptionStore>) -> Router {
    Router::new()
        .route("/api/subscriptions/export", get(export))
        .route("/api/subscriptions/import/preview", post(preview))
        .route("/api/subscriptions/import", post(import))
        .with_state(store)
}
