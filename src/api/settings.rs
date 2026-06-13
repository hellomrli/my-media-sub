use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::Result;
use crate::store::SettingsStore;

/// 设置路由状态
pub struct SettingsState {
    pub store: Arc<SettingsStore>,
}

/// 更新设置请求
#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    #[serde(flatten)]
    pub settings: serde_json::Value,
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
) -> Result<Json<Response<crate::models::Settings>>> {
    let settings = state.store.get().await;
    // TODO: 脱敏处理（将密钥替换为空字符串）
    Ok(Json(Response::ok(settings)))
}

/// 更新设置
async fn update_settings(
    State(state): State<Arc<SettingsState>>,
    Json(req): Json<serde_json::Map<String, serde_json::Value>>,
) -> Result<impl IntoResponse> {
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
                    "subscription_check_interval_minutes" => {
                        if let Some(n) = value.as_i64() {
                            settings.subscription_check_interval_minutes = n as i32;
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
                        if let Some(s) = value.as_str() {
                            settings.quark_cookie = s.to_string();
                        }
                    }
                    "wecom_bot_url" => {
                        if let Some(s) = value.as_str() {
                            settings.wecom_bot_url = s.to_string();
                        }
                    }
                    "wxpusher_app_token" => {
                        if let Some(s) = value.as_str() {
                            settings.wxpusher_app_token = s.to_string();
                        }
                    }
                    "telegram_bot_token" => {
                        if let Some(s) = value.as_str() {
                            settings.telegram_bot_token = s.to_string();
                        }
                    }
                    "telegram_chat_id" => {
                        if let Some(s) = value.as_str() {
                            settings.telegram_chat_id = s.to_string();
                        }
                    }
                    _ => {} // 忽略未知字段
                }
            }
        })
        .await?;

    Ok(Json(Response::ok(updated)))
}

/// 创建设置路由
pub fn routes(store: Arc<SettingsStore>) -> Router {
    let state = Arc::new(SettingsState { store });

    Router::new()
        .route("/api/settings", get(get_settings))
        .route("/api/settings", post(update_settings))
        .with_state(state)
}
