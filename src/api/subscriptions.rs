use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::models::Subscription;
use crate::services::{CheckResult, SubscriptionCheckService, SubscriptionTransferService};
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};

/// 订阅路由状态
pub struct SubscriptionState {
    pub store: Arc<SubscriptionStore>,
    pub settings_store: Arc<SettingsStore>,
    pub notification_store: Arc<NotificationStore>,
    pub check_service: Arc<SubscriptionCheckService>,
}

/// 创建订阅请求
#[derive(Debug, Deserialize)]
pub struct CreateSubscriptionRequest {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub media_type: String,
    #[serde(default)]
    pub season: i32,
    #[serde(default)]
    pub cloud_type: String,
    #[serde(default)]
    pub target_dir: String,
    #[serde(default)]
    pub target_fid: String,
    #[serde(default)]
    pub rename_template: String,
    #[serde(default)]
    pub notify_only: bool,
}

/// 更新订阅请求
#[derive(Debug, Deserialize)]
pub struct UpdateSubscriptionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify_only: Option<bool>,
}

/// 通用响应
#[derive(Serialize)]
struct Response<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl<T> Response<T> {
    fn ok(data: T) -> Self {
        Self {
            data: Some(data),
            message: None,
        }
    }

    fn error(message: String) -> Response<()> {
        Response {
            data: None,
            message: Some(message),
        }
    }
}

/// 列出所有订阅
async fn list_subscriptions(
    State(state): State<Arc<SubscriptionState>>,
) -> Result<Json<Response<Vec<Subscription>>>> {
    let subscriptions = state.store.list().await;
    Ok(Json(Response::ok(subscriptions)))
}

/// 获取单个订阅
async fn get_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    match state.store.get(&id).await {
        Some(sub) => Ok(Json(Response::ok(sub))),
        None => Err(AppError::NotFound("订阅不存在".to_string())),
    }
}

/// 创建订阅
async fn create_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Json(req): Json<CreateSubscriptionRequest>,
) -> Result<impl IntoResponse> {
    let id = format!("{:x}", md5::compute(format!("{}:{}", req.url, req.title)));
    let id = &id[..12];

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let subscription = Subscription {
        id: id.to_string(),
        title: req.title,
        source_title: String::new(),
        media_type: if req.media_type.is_empty() {
            "series".to_string()
        } else {
            req.media_type
        },
        season: if req.season > 0 { req.season } else { 1 },
        current_episode_number: 0,
        total_episode_number: None,
        source_group: String::new(),
        cloud_type: if req.cloud_type.is_empty() {
            "quark".to_string()
        } else {
            req.cloud_type
        },
        url: req.url,
        password: req.password,
        known_files: vec![],
        known_file_keys: vec![],
        known_episodes: vec![],
        transferred_files: vec![],
        transferred_file_keys: vec![],
        last_probe: None,
        last_plan_summary: String::new(),
        notify_only: req.notify_only,
        enabled: true,
        completed: false,
        rules: crate::models::rules::TransferRules {
            target_dir: req.target_dir,
            rename_template: req.rename_template,
            ..Default::default()
        },
        created_at: now,
        updated_at: now,
        last_checked_at: now,
        last_new_files: vec![],
        last_new_episodes: vec![],
        last_check_summary: String::new(),
        check_history: vec![],
        status: "active".to_string(),
        invalid_since: None,
        last_error: String::new(),
        rule_summary: String::new(),
    };

    let created = state.store.create(subscription).await?;
    Ok((StatusCode::CREATED, Json(Response::ok(created))))
}

/// 更新订阅
async fn update_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSubscriptionRequest>,
) -> Result<impl IntoResponse> {
    let updated = state
        .store
        .update(&id, |sub| {
            if let Some(title) = req.title {
                sub.title = title;
            }
            if let Some(enabled) = req.enabled {
                sub.enabled = enabled;
            }
            if let Some(notify_only) = req.notify_only {
                sub.notify_only = notify_only;
            }
            sub.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
        })
        .await?;

    match updated {
        Some(sub) => Ok(Json(Response::ok(sub))),
        None => Err(AppError::NotFound("订阅不存在".to_string())),
    }
}

/// 删除订阅
async fn delete_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let deleted = state.store.delete(&id).await?;
    if deleted {
        Ok((StatusCode::NO_CONTENT, ()))
    } else {
        Err(AppError::NotFound("订阅不存在".to_string()))
    }
}

/// 检查单个订阅
async fn check_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie;

    if cookie.is_empty() {
        return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
    }

    let result = state.check_service.check_subscription(&id, &cookie).await?;

    Ok(Json(Response::ok(CheckResponse {
        subscription_id: result.subscription_id,
        new_files: result.new_files,
        new_episodes: result.new_episodes,
        became_invalid: result.became_invalid,
        summary: result.summary,
    })))
}

/// 检查所有订阅
async fn check_all_subscriptions(
    State(state): State<Arc<SubscriptionState>>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie;

    if cookie.is_empty() {
        return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
    }

    let results = state.check_service.check_all_subscriptions(&cookie).await?;

    let responses: Vec<CheckResponse> = results
        .into_iter()
        .map(|r| CheckResponse {
            subscription_id: r.subscription_id,
            new_files: r.new_files,
            new_episodes: r.new_episodes,
            became_invalid: r.became_invalid,
            summary: r.summary,
        })
        .collect();

    Ok(Json(Response::ok(responses)))
}

/// 检查响应
#[derive(Serialize)]
struct CheckResponse {
    subscription_id: String,
    new_files: Vec<String>,
    new_episodes: Vec<i32>,
    became_invalid: bool,
    summary: String,
}

/// 创建订阅路由
pub fn routes(
    store: Arc<SubscriptionStore>,
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
) -> Router {
    let transfer_service = Arc::new(SubscriptionTransferService::new(
        store.clone(),
        settings_store.clone(),
        notification_store.clone(),
    ));

    let check_service = Arc::new(
        SubscriptionCheckService::new(store.clone(), notification_store.clone())
            .with_transfer_service(transfer_service),
    );

    let state = Arc::new(SubscriptionState {
        store,
        settings_store,
        notification_store,
        check_service,
    });

    Router::new()
        .route("/api/subscriptions", get(list_subscriptions))
        .route("/api/subscriptions", post(create_subscription))
        .route("/api/subscriptions/check", post(check_all_subscriptions))
        .route("/api/subscriptions/{id}", get(get_subscription))
        .route("/api/subscriptions/{id}", put(update_subscription))
        .route("/api/subscriptions/{id}", delete(delete_subscription))
        .route("/api/subscriptions/{id}/check", post(check_subscription))
        .with_state(state)
}
