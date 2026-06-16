use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;

use crate::error::Result;
use crate::models::CustomCategory;
use crate::services::SubscriptionScheduler;
use crate::store::{
    settings::{SECRET_KEYS, SUPPORTED_CLOUD_TYPES},
    SettingsStore,
};

/// 设置路由状态
pub struct SettingsState {
    pub store: Arc<SettingsStore>,
    pub scheduler: Arc<SubscriptionScheduler>,
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
            obj.insert((*key).to_string(), serde_json::Value::String(String::new()));
        }

        obj.insert(
            "supported_cloud_types".to_string(),
            serde_json::json!(SUPPORTED_CLOUD_TYPES),
        );
    }

    Ok(value)
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
                        if let Some(s) = non_empty_string(&value) {
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
                    "metadata_provider" => {
                        if let Some(s) = string_value(&value) {
                            settings.metadata_provider = s;
                        }
                    }
                    "tmdb_api_key" => {
                        if let Some(s) = non_empty_string(&value) {
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
                        if let Some(s) = non_empty_string(&value) {
                            settings.quark_cookie = s;
                        }
                    }
                    "quark_save_enabled" => {
                        if let Some(b) = value.as_bool() {
                            settings.quark_save_enabled = b;
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
                        if let Some(s) = non_empty_string(&value) {
                            settings.aria2_secret = s;
                        }
                    }
                    "aria2_dir" => {
                        if let Some(s) = string_value(&value) {
                            settings.aria2_dir = s;
                        }
                    }
                    "nas_sync_enabled" => {
                        if let Some(b) = value.as_bool() {
                            settings.nas_sync_enabled = b;
                        }
                    }
                    "nas_sync_source" => {
                        if let Some(s) = string_value(&value) {
                            settings.nas_sync_source = s;
                        }
                    }
                    "nas_sync_target" => {
                        if let Some(s) = string_value(&value) {
                            settings.nas_sync_target = s;
                        }
                    }
                    "wecom_bot_url" => {
                        if let Some(s) = value.as_str() {
                            settings.wecom_bot_url = s.to_string();
                        }
                    }
                    "wxpusher_app_token" => {
                        if let Some(s) = non_empty_string(&value) {
                            settings.wxpusher_app_token = s;
                        }
                    }
                    "wxpusher_uids" => {
                        if let Some(s) = string_value(&value) {
                            settings.wxpusher_uids = s;
                        }
                    }
                    "telegram_bot_token" => {
                        if let Some(s) = non_empty_string(&value) {
                            settings.telegram_bot_token = s;
                        }
                    }
                    "telegram_chat_id" => {
                        if let Some(s) = value.as_str() {
                            settings.telegram_chat_id = s.to_string();
                        }
                    }
                    "bark_url" => {
                        if let Some(s) = string_value(&value) {
                            settings.bark_url = s;
                        }
                    }
                    "gotify_url" => {
                        if let Some(s) = string_value(&value) {
                            settings.gotify_url = s;
                        }
                    }
                    "gotify_token" => {
                        if let Some(s) = non_empty_string(&value) {
                            settings.gotify_token = s;
                        }
                    }
                    "pushplus_token" => {
                        if let Some(s) = non_empty_string(&value) {
                            settings.pushplus_token = s;
                        }
                    }
                    "serverchan_key" => {
                        if let Some(s) = non_empty_string(&value) {
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

    Ok(Json(Response::ok(public_settings(updated)?)))
}

/// 创建设置路由
pub fn routes(store: Arc<SettingsStore>, scheduler: Arc<SubscriptionScheduler>) -> Router {
    let state = Arc::new(SettingsState { store, scheduler });

    Router::new()
        .route("/api/settings", get(get_settings))
        .route("/api/settings", post(update_settings))
        .with_state(state)
}
