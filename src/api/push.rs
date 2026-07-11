use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use super::response::{json_ok, ApiResponse as Response};
use crate::error::Result;
use crate::services::push::{record_push_message_with_errors, PushEvent, PushLevel, PushService};
use crate::store::{NotificationStore, SettingsStore};

/// 推送路由状态
pub struct PushState {
    pub settings_store: Arc<SettingsStore>,
    pub notification_store: Arc<NotificationStore>,
}

#[derive(Debug, Deserialize)]
struct BrowserSubscriptionRequest {
    endpoint: String,
    p256dh: String,
    auth: String,
    #[serde(default)]
    user_agent: String,
}

#[derive(Debug, Deserialize)]
struct BrowserUnsubscribeRequest {
    endpoint: String,
}

#[derive(Debug, Serialize)]
struct BrowserPushStatus {
    public_key: String,
    subscriptions: usize,
}

async fn browser_push_status(
    State(state): State<Arc<PushState>>,
) -> Result<Json<Response<BrowserPushStatus>>> {
    let settings = state.settings_store.get().await;
    Ok(json_ok(BrowserPushStatus {
        public_key: settings.browser_push_vapid_public_key,
        subscriptions: settings.browser_push_subscriptions.len(),
    }))
}

async fn subscribe_browser_push(
    State(state): State<Arc<PushState>>,
    Json(request): Json<BrowserSubscriptionRequest>,
) -> Result<Json<Response<BrowserPushStatus>>> {
    if !request.endpoint.starts_with("https://")
        || request.endpoint.len() > 2048
        || request.p256dh.len() > 256
        || request.auth.len() > 128
    {
        return Err(crate::error::AppError::Validation(
            "Browser Push 订阅参数无效".to_string(),
        ));
    }
    let endpoint = request.endpoint.clone();
    let updated = state
        .settings_store
        .update(|settings| {
            settings
                .browser_push_subscriptions
                .retain(|item| item.endpoint != endpoint);
            settings
                .browser_push_subscriptions
                .push(crate::models::BrowserPushSubscription {
                    endpoint: request.endpoint,
                    p256dh: request.p256dh,
                    auth: request.auth,
                    user_agent: request.user_agent.chars().take(256).collect(),
                    created_at: crate::utils::unix_now(),
                });
            if settings.browser_push_subscriptions.len() > 20 {
                settings
                    .browser_push_subscriptions
                    .drain(0..settings.browser_push_subscriptions.len() - 20);
            }
        })
        .await?;
    Ok(json_ok(BrowserPushStatus {
        public_key: updated.browser_push_vapid_public_key,
        subscriptions: updated.browser_push_subscriptions.len(),
    }))
}

async fn unsubscribe_browser_push(
    State(state): State<Arc<PushState>>,
    Json(request): Json<BrowserUnsubscribeRequest>,
) -> Result<Json<Response<BrowserPushStatus>>> {
    let endpoint = request.endpoint;
    let updated = state
        .settings_store
        .update(|settings| {
            settings
                .browser_push_subscriptions
                .retain(|item| item.endpoint != endpoint)
        })
        .await?;
    Ok(json_ok(BrowserPushStatus {
        public_key: updated.browser_push_vapid_public_key,
        subscriptions: updated.browser_push_subscriptions.len(),
    }))
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

    /// 失败原因（渠道名 -> 已脱敏错误）
    errors: HashMap<String, String>,

    /// 成功数量
    success_count: usize,

    /// 失败数量
    failed_count: usize,
}

#[derive(Debug, Deserialize)]
struct TemplatePreviewRequest {
    event: String,
    title: String,
    message: String,
    level: String,
}

#[derive(Debug, Serialize)]
struct TemplatePreviewResponse {
    title: String,
    message: String,
    channels: Vec<String>,
}

async fn preview_template(
    State(state): State<Arc<PushState>>,
    Json(request): Json<TemplatePreviewRequest>,
) -> Result<Json<Response<TemplatePreviewResponse>>> {
    let event = PushEvent::from_name(&request.event)
        .ok_or_else(|| crate::error::AppError::Validation("未知推送事件".to_string()))?;
    let level = PushLevel::from_name(&request.level)
        .ok_or_else(|| crate::error::AppError::Validation("未知推送级别".to_string()))?;
    let service = PushService::new(state.settings_store.get().await);
    let (title, message) = service.render_template(event, &request.title, &request.message, level);
    Ok(json_ok(TemplatePreviewResponse {
        title,
        message,
        channels: service.channels_for_event(event, level),
    }))
}

#[derive(Debug, Deserialize)]
struct RotateWebhookSecretRequest {
    #[serde(default = "default_overlap_hours")]
    overlap_hours: i64,
}
fn default_overlap_hours() -> i64 {
    24
}

#[derive(Debug, Serialize)]
struct RotateWebhookSecretResponse {
    secret: String,
    previous_expires_at: i64,
}

async fn rotate_webhook_secret(
    State(state): State<Arc<PushState>>,
    Json(request): Json<RotateWebhookSecretRequest>,
) -> Result<Json<Response<RotateWebhookSecretResponse>>> {
    let secret = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let expires = crate::utils::unix_now() + request.overlap_hours.clamp(1, 168) * 3600;
    let returned = secret.clone();
    state
        .settings_store
        .update(|settings| {
            settings.webhook_previous_secret = std::mem::take(&mut settings.webhook_secret);
            settings.webhook_previous_secret_expires_at = expires;
            settings.webhook_secret = secret;
        })
        .await?;
    Ok(json_ok(RotateWebhookSecretResponse {
        secret: returned,
        previous_expires_at: expires,
    }))
}

#[derive(Debug, Serialize)]
struct PushDiagnosticsResponse {
    enabled_channels: Vec<String>,
    event_routes: std::collections::HashMap<String, Vec<String>>,
    minimum_level: String,
    quiet_hours_enabled: bool,
    quiet_hours: String,
    digest_enabled: bool,
    dedup_window_seconds: i64,
    webhook_rotation_active: bool,
}

async fn push_diagnostics(
    State(state): State<Arc<PushState>>,
) -> Result<Json<Response<PushDiagnosticsResponse>>> {
    let settings = state.settings_store.get().await;
    let service = PushService::new(settings.clone());
    Ok(json_ok(PushDiagnosticsResponse {
        enabled_channels: service.enabled_channels(),
        event_routes: settings.push_event_routes,
        minimum_level: settings.push_min_level,
        quiet_hours_enabled: settings.push_quiet_hours_enabled,
        quiet_hours: format!(
            "{:02}:00-{:02}:00",
            settings.push_quiet_start_hour, settings.push_quiet_end_hour
        ),
        digest_enabled: settings.push_digest_enabled,
        dedup_window_seconds: settings.push_dedup_window_seconds,
        webhook_rotation_active: !settings.webhook_previous_secret.is_empty()
            && settings.webhook_previous_secret_expires_at >= crate::utils::unix_now(),
    }))
}

/// 推送测试 API
async fn test_push(
    State(state): State<Arc<PushState>>,
    Json(req): Json<PushTestRequest>,
) -> Result<Json<Response<PushTestResponse>>> {
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
        req.channels.clone()
    };

    // 准备测试消息
    let title = req.title.unwrap_or_else(|| "推送测试".to_string());
    let message = req.message.unwrap_or_else(|| {
        "这是一条来自 my-media-sub 的测试消息。如果你收到此消息，说明推送配置正常工作！".to_string()
    });

    // 发送测试推送
    let report = push_service
        .send_to_channels_detailed(&test_channels, &title, &message, PushLevel::Info)
        .await;
    record_push_message_with_errors(
        &state.notification_store,
        "push_test",
        &title,
        &message,
        PushLevel::Info,
        &report.results,
        &report.errors,
    )
    .await;

    // 统计结果
    let success_count = report.results.values().filter(|&&v| v).count();
    let failed_count = report.results.len() - success_count;

    Ok(json_ok(PushTestResponse {
        enabled_channels,
        results: report.results,
        errors: report.errors,
        success_count,
        failed_count,
    }))
}

/// 获取推送状态
async fn push_status(
    State(state): State<Arc<PushState>>,
) -> Result<Json<Response<PushStatusResponse>>> {
    let settings = state.settings_store.get().await;
    let push_service = PushService::new(settings.clone());

    let enabled_channels = push_service.enabled_channels();

    // 收集各渠道的配置状态
    let mut channel_configs = HashMap::new();

    // Telegram
    if !settings.telegram_bot_token.is_empty() && !settings.telegram_chat_id.is_empty() {
        channel_configs.insert(
            "telegram".to_string(),
            ChannelConfig {
                enabled: true,
                configured: true,
            },
        );
    }

    // Bark
    if !settings.bark_url.is_empty() {
        channel_configs.insert(
            "bark".to_string(),
            ChannelConfig {
                enabled: true,
                configured: true,
            },
        );
    }

    // Server酱
    if !settings.serverchan_key.is_empty() {
        channel_configs.insert(
            "serverchan".to_string(),
            ChannelConfig {
                enabled: true,
                configured: true,
            },
        );
    }

    // 企业微信
    if !settings.wecom_bot_url.is_empty() {
        channel_configs.insert(
            "wecom".to_string(),
            ChannelConfig {
                enabled: true,
                configured: true,
            },
        );
    }

    // WxPusher
    if !settings.wxpusher_app_token.is_empty() {
        channel_configs.insert(
            "wxpusher".to_string(),
            ChannelConfig {
                enabled: true,
                configured: true,
            },
        );
    }

    // Gotify
    if !settings.gotify_url.is_empty() && !settings.gotify_token.is_empty() {
        channel_configs.insert(
            "gotify".to_string(),
            ChannelConfig {
                enabled: true,
                configured: true,
            },
        );
    }

    // PushPlus
    if !settings.pushplus_token.is_empty() {
        channel_configs.insert(
            "pushplus".to_string(),
            ChannelConfig {
                enabled: true,
                configured: true,
            },
        );
    }

    Ok(json_ok(PushStatusResponse {
        enabled_channels,
        channel_configs,
        push_on_update: settings.push_on_update,
        push_on_failed: settings.push_on_failed,
        push_on_completed: settings.push_on_completed,
        push_on_save: settings.push_on_save,
        push_on_download_completed: settings.push_on_download_completed,
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
    push_on_download_completed: bool,
    push_silent: bool,
}

/// 创建推送路由
pub fn routes(
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
) -> Router {
    let state = Arc::new(PushState {
        settings_store,
        notification_store,
    });

    Router::new()
        .route("/api/push/test", post(test_push))
        .route("/api/push/diagnostics", get(push_diagnostics))
        .route("/api/push/template/preview", post(preview_template))
        .route(
            "/api/push/webhook/rotate-secret",
            post(rotate_webhook_secret),
        )
        .route(
            "/api/push/browser",
            get(browser_push_status)
                .post(subscribe_browser_push)
                .delete(unsubscribe_browser_push),
        )
        .route("/api/push/status", axum::routing::get(push_status))
        .with_state(state)
}
