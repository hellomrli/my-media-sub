use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;

use crate::error::Result;
use crate::models::CustomCategory;
use crate::services::{QuarkSigninScheduler, SubscriptionScheduler};
use crate::store::{
    settings::{SECRET_KEYS, SUPPORTED_CLOUD_TYPES},
    SettingsStore,
};

/// 设置路由状态
pub struct SettingsState {
    pub store: Arc<SettingsStore>,
    pub scheduler: Arc<SubscriptionScheduler>,
    pub quark_signin_scheduler: Arc<QuarkSigninScheduler>,
}

/// 通用响应
#[derive(Serialize)]
struct Response<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
}

impl<T> Response<T> {
    fn ok(data: T) -> Self {
        Self { data: Some(data) }
    }
}

#[derive(Serialize)]
struct SecretFieldResponse {
    key: String,
    value: String,
}

/// 获取设置（公开视图，脱敏密钥）
async fn get_settings(
    State(state): State<Arc<SettingsState>>,
) -> Result<Json<Response<serde_json::Value>>> {
    let settings = state.store.get().await;
    Ok(Json(Response::ok(public_settings(settings)?)))
}

fn public_settings(settings: crate::models::Settings) -> Result<serde_json::Value> {
    let mut value = serde_json::to_value(settings)?;

    if let Some(obj) = value.as_object_mut() {
        for key in SECRET_KEYS {
            let configured = obj
                .get(*key)
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            obj.insert(
                format!("{}_configured", key),
                serde_json::Value::Bool(configured),
            );
            let masked = obj
                .get(*key)
                .and_then(|v| v.as_str())
                .map(mask_secret)
                .unwrap_or_default();
            obj.insert((*key).to_string(), serde_json::Value::String(masked));
        }

        obj.insert(
            "supported_cloud_types".to_string(),
            serde_json::json!(SUPPORTED_CLOUD_TYPES),
        );
        obj.remove("nas_sync_enabled");
        obj.remove("nas_sync_source");
        obj.remove("nas_sync_target");
    }

    Ok(value)
}

fn mask_secret(value: &str) -> String {
    "*".repeat(value.chars().count())
}

fn is_secret_mask(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|ch| ch == '*')
}

fn non_mask_secret(value: &serde_json::Value) -> Option<String> {
    non_empty_string(value).filter(|s| !is_secret_mask(s))
}

fn setting_secret(settings: &crate::models::Settings, key: &str) -> Option<String> {
    let value = match key {
        "app_password" => &settings.app_password,
        "aria2_secret" => &settings.aria2_secret,
        "quark_cookie" => &settings.quark_cookie,
        "strm_access_token" => &settings.strm_access_token,
        "pansou_api_url" => &settings.pansou_api_url,
        "tmdb_api_key" => &settings.tmdb_api_key,
        "wecom_bot_url" => &settings.wecom_bot_url,
        "bark_url" => &settings.bark_url,
        "wxpusher_app_token" => &settings.wxpusher_app_token,
        "telegram_bot_token" => &settings.telegram_bot_token,
        "gotify_token" => &settings.gotify_token,
        "pushplus_token" => &settings.pushplus_token,
        "serverchan_key" => &settings.serverchan_key,
        _ => return None,
    };
    Some(value.clone())
}

async fn get_setting_secret(
    State(state): State<Arc<SettingsState>>,
    Path(key): Path<String>,
) -> Result<Json<Response<SecretFieldResponse>>> {
    if !SECRET_KEYS.contains(&key.as_str()) {
        return Err(crate::error::AppError::NotFound(
            "设置字段不存在".to_string(),
        ));
    }

    let settings = state.store.get().await;
    let value = setting_secret(&settings, &key).unwrap_or_default();
    Ok(Json(Response::ok(SecretFieldResponse { key, value })))
}

fn non_empty_string(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn string_value(value: &serde_json::Value) -> Option<String> {
    value.as_str().map(ToString::to_string)
}

/// 更新设置
async fn update_settings(
    State(state): State<Arc<SettingsState>>,
    Json(req): Json<serde_json::Map<String, serde_json::Value>>,
) -> Result<impl IntoResponse> {
    let previous = state.store.get().await;

    let updated = state
        .store
        .update(|settings| {
            // 只更新允许的字段
            for (key, value) in req {
                match key.as_str() {
                    "app_username" => {
                        if let Some(s) = value.as_str() {
                            settings.app_username = s.to_string();
                        }
                    }
                    "app_password" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.app_password = s;
                        }
                    }
                    "check_links" => {
                        if let Some(b) = value.as_bool() {
                            settings.check_links = b;
                        }
                    }
                    "probe_quark_files" => {
                        if let Some(b) = value.as_bool() {
                            settings.probe_quark_files = b;
                        }
                    }
                    "filter_bad_links" => {
                        if let Some(b) = value.as_bool() {
                            settings.filter_bad_links = b;
                        }
                    }
                    "pansou_api_url" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.pansou_api_url = s;
                        }
                    }
                    "metadata_provider" => {
                        if let Some(s) = string_value(&value) {
                            settings.metadata_provider = s;
                        }
                    }
                    "tmdb_api_key" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.tmdb_api_key = s;
                        }
                    }
                    "tmdb_language" => {
                        if let Some(s) = string_value(&value) {
                            settings.tmdb_language = s;
                        }
                    }
                    "subscription_check_interval_minutes" => {
                        if let Some(n) = value.as_i64() {
                            settings.subscription_check_interval_minutes = n as i32;
                        }
                    }
                    "subscription_scheduler_enabled" => {
                        if let Some(b) = value.as_bool() {
                            settings.subscription_scheduler_enabled = b;
                        }
                    }
                    "auto_download_new_subscription_items" => {
                        if let Some(b) = value.as_bool() {
                            settings.auto_download_new_subscription_items = b;
                        }
                    }
                    "cloud_types" => {
                        if let Some(arr) = value.as_array() {
                            settings.cloud_types = arr
                                .iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect();
                        }
                    }
                    "quark_cookie" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.quark_cookie = s;
                        }
                    }
                    "quark_save_enabled" => {
                        if let Some(b) = value.as_bool() {
                            settings.quark_save_enabled = b;
                        }
                    }
                    "quark_signin_enabled" => {
                        if let Some(b) = value.as_bool() {
                            settings.quark_signin_enabled = b;
                        }
                    }
                    "quark_signin_hour" => {
                        if let Some(n) = value.as_i64() {
                            settings.quark_signin_hour = (n as i32).clamp(0, 23);
                        }
                    }
                    "quark_save_root" => {
                        if let Some(s) = string_value(&value) {
                            settings.quark_save_root = s;
                        }
                    }
                    "quark_save_movie_dir" => {
                        if let Some(s) = string_value(&value) {
                            settings.quark_save_movie_dir = s;
                        }
                    }
                    "quark_save_series_dir" => {
                        if let Some(s) = string_value(&value) {
                            settings.quark_save_series_dir = s;
                        }
                    }
                    "quark_save_anime_dir" => {
                        if let Some(s) = string_value(&value) {
                            settings.quark_save_anime_dir = s;
                        }
                    }
                    "custom_categories" => {
                        if let Ok(categories) = serde_json::from_value::<Vec<CustomCategory>>(value)
                        {
                            settings.custom_categories = categories;
                        }
                    }
                    "aria2_rpc_url" => {
                        if let Some(s) = string_value(&value) {
                            settings.aria2_rpc_url = s;
                        }
                    }
                    "aria2_secret" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.aria2_secret = s;
                        }
                    }
                    "aria2_movie_dir" => {
                        if let Some(s) = string_value(&value) {
                            settings.aria2_movie_dir = s;
                        }
                    }
                    "aria2_series_dir" => {
                        if let Some(s) = string_value(&value) {
                            settings.aria2_series_dir = s;
                        }
                    }
                    "aria2_anime_dir" => {
                        if let Some(s) = string_value(&value) {
                            settings.aria2_anime_dir = s;
                        }
                    }
                    "strm_enabled" => {
                        if let Some(b) = value.as_bool() {
                            settings.strm_enabled = b;
                        }
                    }
                    "strm_output_dir" => {
                        if let Some(s) = string_value(&value) {
                            settings.strm_output_dir = s;
                        }
                    }
                    "strm_public_base_url" => {
                        if let Some(s) = string_value(&value) {
                            settings.strm_public_base_url = s;
                        }
                    }
                    "strm_access_token" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.strm_access_token = s;
                        }
                    }
                    "wecom_bot_url" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.wecom_bot_url = s.to_string();
                        }
                    }
                    "wxpusher_app_token" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.wxpusher_app_token = s;
                        }
                    }
                    "wxpusher_uids" => {
                        if let Some(s) = string_value(&value) {
                            settings.wxpusher_uids = s;
                        }
                    }
                    "telegram_bot_token" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.telegram_bot_token = s;
                        }
                    }
                    "telegram_chat_id" => {
                        if let Some(s) = value.as_str() {
                            settings.telegram_chat_id = s.to_string();
                        }
                    }
                    "bark_url" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.bark_url = s;
                        }
                    }
                    "gotify_url" => {
                        if let Some(s) = string_value(&value) {
                            settings.gotify_url = s;
                        }
                    }
                    "gotify_token" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.gotify_token = s;
                        }
                    }
                    "pushplus_token" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.pushplus_token = s;
                        }
                    }
                    "serverchan_key" => {
                        if let Some(s) = non_mask_secret(&value) {
                            settings.serverchan_key = s;
                        }
                    }
                    "push_on_update" => {
                        if let Some(b) = value.as_bool() {
                            settings.push_on_update = b;
                        }
                    }
                    "push_on_failed" => {
                        if let Some(b) = value.as_bool() {
                            settings.push_on_failed = b;
                        }
                    }
                    "push_on_completed" => {
                        if let Some(b) = value.as_bool() {
                            settings.push_on_completed = b;
                        }
                    }
                    "push_on_save" => {
                        if let Some(b) = value.as_bool() {
                            settings.push_on_save = b;
                        }
                    }
                    "push_on_download_completed" => {
                        if let Some(b) = value.as_bool() {
                            settings.push_on_download_completed = b;
                        }
                    }
                    "push_on_quark_signin" => {
                        if let Some(b) = value.as_bool() {
                            settings.push_on_quark_signin = b;
                        }
                    }
                    "push_silent" => {
                        if let Some(b) = value.as_bool() {
                            settings.push_silent = b;
                        }
                    }
                    _ => {} // 忽略未知字段
                }
            }
        })
        .await?;

    if previous.subscription_scheduler_enabled != updated.subscription_scheduler_enabled
        || previous.subscription_check_interval_minutes
            != updated.subscription_check_interval_minutes
        || previous.quark_cookie != updated.quark_cookie
    {
        state.scheduler.reload().await?;
    }
    if previous.quark_signin_enabled != updated.quark_signin_enabled
        || previous.quark_signin_hour != updated.quark_signin_hour
        || previous.quark_cookie != updated.quark_cookie
    {
        state.quark_signin_scheduler.reload().await?;
    }

    Ok(Json(Response::ok(public_settings(updated)?)))
}

/// 创建设置路由
pub fn routes(
    store: Arc<SettingsStore>,
    scheduler: Arc<SubscriptionScheduler>,
    quark_signin_scheduler: Arc<QuarkSigninScheduler>,
) -> Router {
    let state = Arc::new(SettingsState {
        store,
        scheduler,
        quark_signin_scheduler,
    });

    Router::new()
        .route("/api/settings", get(get_settings))
        .route("/api/settings", post(update_settings))
        .route("/api/settings/secret/{key}", get(get_setting_secret))
        .with_state(state)
}
