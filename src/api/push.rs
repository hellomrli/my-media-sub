use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::Result;
use crate::services::push::{PushLevel, PushService};
use crate::store::SettingsStore;

/// 推送路由状态
pub struct PushState {
    pub settings_store: Arc<SettingsStore>,
}

/// 推送测试请求
#[derive(Debug, Deserialize)]
struct PushTestRequest {
    /// 要测试的渠道（可选，不指定则测试所有已配置的渠道）
    #[serde(default)]
    channels: Vec<String>,

    /// 自定义测试标题（可选）
    #[serde(default)]
    title: Option<String>,

    /// 自定义测试消息（可选）
    #[serde(default)]
    message: Option<String>,
}

/// 推送测试响应
#[derive(Debug, Serialize)]
struct PushTestResponse {
    /// 已启用的渠道列表
    enabled_channels: Vec<String>,

    /// 测试结果（渠道名 -> 是否成功）
    results: HashMap<String, bool>,

    /// 成功数量
    success_count: usize,

    /// 失败数量
    failed_count: usize,
}

/// 推送测试 API
async fn test_push(
    State(state): State<Arc<PushState>>,
    Json(req): Json<PushTestRequest>,
) -> Result<Json<PushTestResponse>> {
    // 读取当前设置
    let settings = state.settings_store.get().await;

    // 创建推送服务
    let push_service = PushService::new(settings);

    // 获取已启用的渠道
    let enabled_channels = push_service.enabled_channels();

    // 确定要测试的渠道
    let test_channels: Vec<String> = if req.channels.is_empty() {
        enabled_channels.clone()
    } else {
        // 只测试用户指定的渠道（且已启用）
        req.channels
            .iter()
            .filter(|c| enabled_channels.contains(c))
            .cloned()
            .collect()
    };

    // 准备测试消息
    let title = req
        .title
        .unwrap_or_else(|| "推送测试".to_string());
    let message = req
        .message
        .unwrap_or_else(|| "这是一条来自 my-media-sub 的测试消息。如果你收到此消息，说明推送配置正常工作！".to_string());

    // 发送测试推送
    let results = push_service.send(&title, &message, PushLevel::Info).await;

    // 统计结果
    let success_count = results.values().filter(|&&v| v).count();
    let failed_count = results.len() - success_count;

    Ok(Json(PushTestResponse {
        enabled_channels,
        results,
        success_count,
        failed_count,
    }))
}

/// 获取推送状态
async fn push_status(State(state): State<Arc<PushState>>) -> Result<Json<PushStatusResponse>> {
    let settings = state.settings_store.get().await;
    let push_service = PushService::new(settings.clone());

    let enabled_channels = push_service.enabled_channels();

    // 收集各渠道的配置状态
    let mut channel_configs = HashMap::new();

    // Telegram
    if !settings.telegram_bot_token.is_empty() && !settings.telegram_chat_id.is_empty() {
        channel_configs.insert("telegram".to_string(), ChannelConfig {
            enabled: true,
            configured: true,
        });
    }

    // Bark
    if !settings.bark_url.is_empty() {
        channel_configs.insert("bark".to_string(), ChannelConfig {
            enabled: true,
            configured: true,
        });
    }

    // Server酱
    if !settings.serverchan_key.is_empty() {
        channel_configs.insert("serverchan".to_string(), ChannelConfig {
            enabled: true,
            configured: true,
        });
    }

    // 企业微信
    if !settings.wecom_bot_url.is_empty() {
        channel_configs.insert("wecom".to_string(), ChannelConfig {
            enabled: true,
            configured: true,
        });
    }

    // WxPusher
    if !settings.wxpusher_app_token.is_empty() {
        channel_configs.insert("wxpusher".to_string(), ChannelConfig {
            enabled: true,
            configured: true,
        });
    }

    // Gotify
    if !settings.gotify_url.is_empty() && !settings.gotify_token.is_empty() {
        channel_configs.insert("gotify".to_string(), ChannelConfig {
            enabled: true,
            configured: true,
        });
    }

    // PushPlus
    if !settings.pushplus_token.is_empty() {
        channel_configs.insert("pushplus".to_string(), ChannelConfig {
            enabled: true,
            configured: true,
        });
    }

    Ok(Json(PushStatusResponse {
        enabled_channels,
        channel_configs,
        push_on_update: settings.push_on_update,
        push_on_failed: settings.push_on_failed,
        push_on_completed: settings.push_on_completed,
        push_on_save: settings.push_on_save,
        push_silent: settings.push_silent,
    }))
}

/// 渠道配置状态
#[derive(Debug, Serialize)]
struct ChannelConfig {
    enabled: bool,
    configured: bool,
}

/// 推送状态响应
#[derive(Debug, Serialize)]
struct PushStatusResponse {
    /// 已启用的渠道列表
    enabled_channels: Vec<String>,

    /// 各渠道配置状态
    channel_configs: HashMap<String, ChannelConfig>,

    /// 推送场景开关
    push_on_update: bool,
    push_on_failed: bool,
    push_on_completed: bool,
    push_on_save: bool,
    push_silent: bool,
}

/// 创建推送路由
pub fn routes(settings_store: Arc<SettingsStore>) -> Router {
    let state = Arc::new(PushState { settings_store });

    Router::new()
        .route("/api/push/test", post(test_push))
        .route("/api/push/status", axum::routing::get(push_status))
        .with_state(state)
}
