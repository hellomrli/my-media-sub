use crate::clients::http_pool;
use crate::error::{AppError, Result};
use crate::models::{Notification, Settings};
use crate::store::NotificationStore;
use crate::utils::metrics::global_metrics;
use regex::Regex;
use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::time::sleep;
use web_push::{
    ContentEncoding, IsahcWebPushClient, SubscriptionInfo, VapidSignatureBuilder, WebPushClient,
    WebPushMessageBuilder,
};

fn hardcoded_regex(pattern: &str) -> Regex {
    Regex::new(pattern)
        .unwrap_or_else(|error| panic!("invalid hard-coded push regex `{pattern}`: {error}"))
}

/// 推送级别
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum PushLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl PushLevel {
    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "info" => Some(Self::Info),
            "success" => Some(Self::Success),
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    fn emoji(&self) -> &str {
        match self {
            Self::Info => "ℹ️",
            Self::Success => "✅",
            Self::Warning => "⚠️",
            Self::Error => "❌",
        }
    }
}

/// 业务推送事件
#[derive(Debug, Clone, Copy)]
pub enum PushEvent {
    SubscriptionUpdated,
    SubscriptionFailed,
    SubscriptionCompleted,
    TransferSaved,
    DownloadCompleted,
    QuarkSignin,
}

impl PushEvent {
    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "subscription_updated" => Some(Self::SubscriptionUpdated),
            "subscription_failed" => Some(Self::SubscriptionFailed),
            "subscription_completed" => Some(Self::SubscriptionCompleted),
            "transfer_saved" => Some(Self::TransferSaved),
            "download_completed" => Some(Self::DownloadCompleted),
            "quark_signin" => Some(Self::QuarkSignin),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::SubscriptionUpdated => "subscription_updated",
            Self::SubscriptionFailed => "subscription_failed",
            Self::SubscriptionCompleted => "subscription_completed",
            Self::TransferSaved => "transfer_saved",
            Self::DownloadCompleted => "download_completed",
            Self::QuarkSignin => "quark_signin",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PushDeliveryReport {
    pub results: HashMap<String, bool>,
    pub errors: HashMap<String, String>,
    pub attempts: HashMap<String, usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct PushRetryPolicy {
    pub max_attempts: usize,
    pub initial_delay: Duration,
    pub max_delay: Duration,
}

impl PushRetryPolicy {
    pub fn single_attempt() -> Self {
        Self {
            max_attempts: 1,
            initial_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        }
    }

    pub fn background_default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(10),
        }
    }

    fn attempts(&self) -> usize {
        self.max_attempts.max(1)
    }

    fn delay_for_retry(&self, retry_index: u32) -> Duration {
        if self.initial_delay.is_zero() {
            return Duration::ZERO;
        }

        let multiplier = 2_u32.saturating_pow(retry_index);
        self.initial_delay
            .saturating_mul(multiplier)
            .min(self.max_delay)
    }
}

type ChannelSendFuture<'a> = Pin<Box<dyn Future<Output = Result<bool>> + Send + 'a>>;

trait PushChannel: Sync {
    fn id(&self) -> &'static str;
    fn is_enabled(&self, settings: &Settings) -> bool;
    fn send<'a>(
        &'a self,
        service: &'a PushService,
        title: &'a str,
        message: &'a str,
        level: PushLevel,
    ) -> ChannelSendFuture<'a>;
}

struct WecomChannel;
struct WxpusherChannel;
struct TelegramChannel;
struct BarkChannel;
struct GotifyChannel;
struct PushplusChannel;
struct ServerchanChannel;
struct BrowserChannel;
struct WebhookChannel;

impl PushChannel for WecomChannel {
    fn id(&self) -> &'static str {
        "wecom"
    }

    fn is_enabled(&self, settings: &Settings) -> bool {
        !settings.wecom_bot_url.is_empty()
    }

    fn send<'a>(
        &'a self,
        service: &'a PushService,
        title: &'a str,
        message: &'a str,
        level: PushLevel,
    ) -> ChannelSendFuture<'a> {
        Box::pin(service.send_wecom(title, message, level))
    }
}

impl PushChannel for WxpusherChannel {
    fn id(&self) -> &'static str {
        "wxpusher"
    }

    fn is_enabled(&self, settings: &Settings) -> bool {
        !settings.wxpusher_app_token.is_empty()
    }

    fn send<'a>(
        &'a self,
        service: &'a PushService,
        title: &'a str,
        message: &'a str,
        _level: PushLevel,
    ) -> ChannelSendFuture<'a> {
        Box::pin(service.send_wxpusher(title, message))
    }
}

impl PushChannel for TelegramChannel {
    fn id(&self) -> &'static str {
        "telegram"
    }

    fn is_enabled(&self, settings: &Settings) -> bool {
        !settings.telegram_bot_token.is_empty() && !settings.telegram_chat_id.is_empty()
    }

    fn send<'a>(
        &'a self,
        service: &'a PushService,
        title: &'a str,
        message: &'a str,
        level: PushLevel,
    ) -> ChannelSendFuture<'a> {
        Box::pin(service.send_telegram(title, message, level, service.settings.push_silent))
    }
}

impl PushChannel for BarkChannel {
    fn id(&self) -> &'static str {
        "bark"
    }

    fn is_enabled(&self, settings: &Settings) -> bool {
        !settings.bark_url.is_empty()
    }

    fn send<'a>(
        &'a self,
        service: &'a PushService,
        title: &'a str,
        message: &'a str,
        level: PushLevel,
    ) -> ChannelSendFuture<'a> {
        Box::pin(service.send_bark(title, message, level))
    }
}

impl PushChannel for GotifyChannel {
    fn id(&self) -> &'static str {
        "gotify"
    }

    fn is_enabled(&self, settings: &Settings) -> bool {
        !settings.gotify_url.is_empty() && !settings.gotify_token.is_empty()
    }

    fn send<'a>(
        &'a self,
        service: &'a PushService,
        title: &'a str,
        message: &'a str,
        level: PushLevel,
    ) -> ChannelSendFuture<'a> {
        Box::pin(service.send_gotify(title, message, level))
    }
}

impl PushChannel for PushplusChannel {
    fn id(&self) -> &'static str {
        "pushplus"
    }

    fn is_enabled(&self, settings: &Settings) -> bool {
        !settings.pushplus_token.is_empty()
    }

    fn send<'a>(
        &'a self,
        service: &'a PushService,
        title: &'a str,
        message: &'a str,
        _level: PushLevel,
    ) -> ChannelSendFuture<'a> {
        Box::pin(service.send_pushplus(title, message))
    }
}

impl PushChannel for ServerchanChannel {
    fn id(&self) -> &'static str {
        "serverchan"
    }

    fn is_enabled(&self, settings: &Settings) -> bool {
        !settings.serverchan_key.is_empty()
    }

    fn send<'a>(
        &'a self,
        service: &'a PushService,
        title: &'a str,
        message: &'a str,
        _level: PushLevel,
    ) -> ChannelSendFuture<'a> {
        Box::pin(service.send_serverchan(title, message))
    }
}

impl PushChannel for BrowserChannel {
    fn id(&self) -> &'static str {
        "browser"
    }
    fn is_enabled(&self, settings: &Settings) -> bool {
        !settings.browser_push_subscriptions.is_empty()
    }
    fn send<'a>(
        &'a self,
        service: &'a PushService,
        title: &'a str,
        message: &'a str,
        level: PushLevel,
    ) -> ChannelSendFuture<'a> {
        Box::pin(service.send_browser_push(title, message, level))
    }
}

impl PushChannel for WebhookChannel {
    fn id(&self) -> &'static str {
        "webhook"
    }
    fn is_enabled(&self, settings: &Settings) -> bool {
        settings.webhook_enabled && !settings.webhook_urls.is_empty()
    }
    fn send<'a>(
        &'a self,
        service: &'a PushService,
        title: &'a str,
        message: &'a str,
        level: PushLevel,
    ) -> ChannelSendFuture<'a> {
        Box::pin(service.send_webhooks(title, message, level))
    }
}

fn push_channels() -> [&'static dyn PushChannel; 9] {
    static WECOM: WecomChannel = WecomChannel;
    static WXPUSHER: WxpusherChannel = WxpusherChannel;
    static TELEGRAM: TelegramChannel = TelegramChannel;
    static BARK: BarkChannel = BarkChannel;
    static GOTIFY: GotifyChannel = GotifyChannel;
    static PUSHPLUS: PushplusChannel = PushplusChannel;
    static SERVERCHAN: ServerchanChannel = ServerchanChannel;
    static BROWSER: BrowserChannel = BrowserChannel;
    static WEBHOOK: WebhookChannel = WebhookChannel;

    [
        &WECOM,
        &WXPUSHER,
        &TELEGRAM,
        &BARK,
        &GOTIFY,
        &PUSHPLUS,
        &SERVERCHAN,
        &BROWSER,
        &WEBHOOK,
    ]
}

fn push_channel_by_id(id: &str) -> Option<&'static dyn PushChannel> {
    push_channels()
        .into_iter()
        .find(|channel| channel.id() == id)
}

include!("push/channel_methods.rs");

/// 推送服务
pub struct PushService {
    settings: Settings,
    client: Client,
}

impl PushService {
    pub fn new(settings: Settings) -> Self {
        let client = http_pool::short_client();

        Self { settings, client }
    }

    /// 获取已启用的推送渠道
    pub fn enabled_channels(&self) -> Vec<String> {
        push_channels()
            .into_iter()
            .filter(|channel| channel.is_enabled(&self.settings))
            .map(|channel| channel.id().to_string())
            .collect()
    }

    pub fn event_enabled(&self, event: PushEvent) -> bool {
        match event {
            PushEvent::SubscriptionUpdated => self.settings.push_on_update,
            PushEvent::SubscriptionFailed => self.settings.push_on_failed,
            PushEvent::SubscriptionCompleted => self.settings.push_on_completed,
            PushEvent::TransferSaved => self.settings.push_on_save,
            PushEvent::DownloadCompleted => self.settings.push_on_download_completed,
            PushEvent::QuarkSignin => self.settings.push_on_quark_signin,
        }
    }

    /// 发送推送到所有启用的渠道
    #[allow(dead_code)]
    pub async fn send(
        &self,
        title: &str,
        message: &str,
        level: PushLevel,
    ) -> HashMap<String, bool> {
        self.send_detailed(title, message, level).await.results
    }

    pub async fn send_detailed(
        &self,
        title: &str,
        message: &str,
        level: PushLevel,
    ) -> PushDeliveryReport {
        let channels = self.enabled_channels();
        self.send_to_channels_detailed(&channels, title, message, level)
            .await
    }

    /// 发送推送到指定渠道
    #[allow(dead_code)]
    pub async fn send_to_channels(
        &self,
        channels: &[String],
        title: &str,
        message: &str,
        level: PushLevel,
    ) -> HashMap<String, bool> {
        self.send_to_channels_detailed(channels, title, message, level)
            .await
            .results
    }

    pub async fn send_to_channels_detailed(
        &self,
        channels: &[String],
        title: &str,
        message: &str,
        level: PushLevel,
    ) -> PushDeliveryReport {
        self.send_to_channels_with_retry_detailed(
            channels,
            title,
            message,
            level,
            PushRetryPolicy::single_attempt(),
        )
        .await
    }

    pub async fn send_to_channels_with_retry_detailed(
        &self,
        channels: &[String],
        title: &str,
        message: &str,
        level: PushLevel,
        retry_policy: PushRetryPolicy,
    ) -> PushDeliveryReport {
        let mut report = PushDeliveryReport::default();
        for channel in channels {
            let Some(channel_impl) = push_channel_by_id(channel)
                .filter(|channel_impl| channel_impl.is_enabled(&self.settings))
            else {
                report.results.insert(channel.clone(), false);
                report
                    .errors
                    .insert(channel.clone(), "渠道未配置或未启用".to_string());
                report.attempts.insert(channel.clone(), 0);
                continue;
            };

            let (success, attempts, last_error) = send_with_retry(retry_policy, || {
                channel_impl.send(self, title, message, level)
            })
            .await;

            report.results.insert(channel.clone(), success);
            report.attempts.insert(channel.clone(), attempts);
            if !success {
                let error = if attempts > 1 {
                    format!("尝试 {} 次后失败: {}", attempts, last_error)
                } else {
                    last_error
                };
                report.errors.insert(channel.clone(), error);
            }
        }

        report
    }

    /// 按全局场景开关发送业务事件推送。
    #[allow(dead_code)]
    pub async fn send_event(
        &self,
        event: PushEvent,
        title: &str,
        message: &str,
        level: PushLevel,
    ) -> HashMap<String, bool> {
        self.send_event_detailed(event, title, message, level)
            .await
            .results
    }

    pub async fn send_event_detailed(
        &self,
        event: PushEvent,
        title: &str,
        message: &str,
        level: PushLevel,
    ) -> PushDeliveryReport {
        if !self.event_enabled(event) {
            return PushDeliveryReport::default();
        }

        self.send_detailed(title, message, level).await
    }

    pub async fn send_event_with_retry_detailed(
        &self,
        event: PushEvent,
        title: &str,
        message: &str,
        level: PushLevel,
        retry_policy: PushRetryPolicy,
    ) -> PushDeliveryReport {
        if !self.event_enabled(event) {
            return PushDeliveryReport::default();
        }

        let channels = self.enabled_channels();
        self.send_to_channels_with_retry_detailed(&channels, title, message, level, retry_policy)
            .await
    }

    async fn send_browser_push(
        &self,
        title: &str,
        message: &str,
        level: PushLevel,
    ) -> Result<bool> {
        let client =
            IsahcWebPushClient::new().map_err(|error| AppError::Http(error.to_string()))?;
        let payload = serde_json::to_vec(
            &json!({"title": title, "body": message, "level": level.as_str(), "url": "/?tab=notifications"}),
        )?;
        let mut succeeded = 0usize;
        for subscription in &self.settings.browser_push_subscriptions {
            let info = SubscriptionInfo::new(
                &subscription.endpoint,
                &subscription.p256dh,
                &subscription.auth,
            );
            let mut signature = VapidSignatureBuilder::from_base64(
                &self.settings.browser_push_vapid_private_key,
                &info,
            )
            .map_err(|error| AppError::Config(format!("Browser Push VAPID 签名失败: {error}")))?;
            signature.add_claim("sub", self.settings.browser_push_subject.clone());
            let signature = signature
                .build()
                .map_err(|error| AppError::Http(error.to_string()))?;
            let mut builder = WebPushMessageBuilder::new(&info);
            builder.set_payload(ContentEncoding::Aes128Gcm, &payload);
            builder.set_ttl(3600);
            builder.set_vapid_signature(signature);
            let push = builder
                .build()
                .map_err(|error| AppError::Http(error.to_string()))?;
            if client.send(push).await.is_ok() {
                succeeded += 1;
            }
        }
        Ok(succeeded > 0)
    }

    async fn send_webhooks(&self, title: &str, message: &str, level: PushLevel) -> Result<bool> {
        let payload = json!({"event": "notification", "title": title, "message": message, "level": level.as_str(), "timestamp": crate::utils::unix_now()});
        let bytes = serde_json::to_vec(&payload)?;
        let signature = if self.settings.webhook_secret.is_empty() {
            String::new()
        } else {
            let key = ring::hmac::Key::new(
                ring::hmac::HMAC_SHA256,
                self.settings.webhook_secret.as_bytes(),
            );
            ring::hmac::sign(&key, &bytes)
                .as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        };
        let mut success = 0usize;
        for url in self.settings.webhook_urls.iter().take(5) {
            let mut request = self
                .client
                .post(url)
                .header("content-type", "application/json")
                .body(bytes.clone());
            if !signature.is_empty() {
                request =
                    request.header("x-media-sub-signature-256", format!("sha256={signature}"));
            }
            if request
                .send()
                .await
                .map(|response| response.status().is_success())
                .unwrap_or(false)
            {
                success += 1;
            }
        }
        Ok(success > 0)
    }

    push_channel_methods!();
}

async fn send_with_retry<F, Fut>(
    retry_policy: PushRetryPolicy,
    mut send_attempt: F,
) -> (bool, usize, String)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<bool>>,
{
    let max_attempts = retry_policy.attempts();
    let mut attempts = 0;
    let mut last_error = "渠道返回失败状态".to_string();

    for attempt_index in 0..max_attempts {
        attempts += 1;
        match send_attempt().await {
            Ok(true) => return (true, attempts, last_error),
            Ok(false) => {
                last_error = "渠道返回失败状态".to_string();
            }
            Err(e) => {
                last_error = sanitize_push_error(&e.to_string());
            }
        }

        if attempt_index + 1 < max_attempts {
            let delay = retry_policy.delay_for_retry(attempt_index as u32);
            if !delay.is_zero() {
                sleep(delay).await;
            }
        }
    }

    (false, attempts, last_error)
}

fn sanitize_push_error(value: &str) -> String {
    static TOKEN_RE: LazyLock<Regex> =
        LazyLock::new(|| hardcoded_regex(r"(?i)(token|key|sendkey|access_token)=([^&\s]+)"));
    static BOT_RE: LazyLock<Regex> =
        LazyLock::new(|| hardcoded_regex(r"(?i)bot[0-9]+:[A-Za-z0-9_-]+"));
    static SERVERCHAN_RE: LazyLock<Regex> = LazyLock::new(|| hardcoded_regex(r"SCT[A-Za-z0-9]+"));

    let sanitized = TOKEN_RE.replace_all(value, "$1=***");
    let sanitized = BOT_RE.replace_all(&sanitized, "bot***");
    let sanitized = SERVERCHAN_RE.replace_all(&sanitized, "SCT***");
    let sanitized = sanitized.to_string();

    const MAX_ERROR_LEN: usize = 300;
    if sanitized.chars().count() > MAX_ERROR_LEN {
        format!(
            "{}...",
            sanitized.chars().take(MAX_ERROR_LEN).collect::<String>()
        )
    } else {
        sanitized
    }
}

#[allow(dead_code)]
pub async fn record_push_message(
    notification_store: &NotificationStore,
    source_event: &str,
    title: &str,
    message: &str,
    level: PushLevel,
    results: &HashMap<String, bool>,
) {
    record_push_message_with_errors(
        notification_store,
        source_event,
        title,
        message,
        level,
        results,
        &HashMap::new(),
    )
    .await;
}

pub async fn record_push_message_with_errors(
    notification_store: &NotificationStore,
    source_event: &str,
    title: &str,
    message: &str,
    level: PushLevel,
    results: &HashMap<String, bool>,
    errors: &HashMap<String, String>,
) {
    let report = PushDeliveryReport {
        results: results.clone(),
        errors: errors.clone(),
        attempts: HashMap::new(),
    };
    record_push_message_report(
        notification_store,
        source_event,
        title,
        message,
        level,
        &report,
    )
    .await;
}

pub async fn record_push_message_report(
    notification_store: &NotificationStore,
    source_event: &str,
    title: &str,
    message: &str,
    level: PushLevel,
    report: &PushDeliveryReport,
) {
    record_push_message_report_for_notification(
        notification_store,
        None,
        source_event,
        title,
        message,
        level,
        report,
    )
    .await;
}

pub async fn record_push_message_report_for_notification(
    notification_store: &NotificationStore,
    notification_id: Option<&str>,
    source_event: &str,
    title: &str,
    message: &str,
    level: PushLevel,
    report: &PushDeliveryReport,
) {
    let results = &report.results;

    if results.is_empty() {
        return;
    }

    let success_count = results.values().filter(|&&ok| ok).count();
    let failed_count = results.len().saturating_sub(success_count);
    global_metrics().add_push_results(success_count as u64, failed_count as u64);
    let record_level = if failed_count > 0 {
        "warning"
    } else {
        level.as_str()
    };

    let push_meta = push_report_meta(source_event, title, message, level, report);

    if let Some(notification_id) = notification_id {
        match notification_store
            .update(notification_id, |notification| {
                notification
                    .meta
                    .insert("push".to_string(), json!(push_meta.clone()));
                if notification.level != "error" && record_level == "warning" {
                    notification.level = "warning".to_string();
                    notification.read = false;
                }
            })
            .await
        {
            Ok(Some(_)) => return,
            Ok(None) => {
                tracing::warn!("待合并的通知不存在，改为写入独立通知: {}", notification_id);
            }
            Err(e) => {
                tracing::warn!("合并推送记录失败: {}", e);
                return;
            }
        }
    }

    let notification = Notification {
        id: uuid::Uuid::new_v4().to_string(),
        level: record_level.to_string(),
        event: source_event.to_string(),
        title: title.to_string(),
        message: message.to_string(),
        meta: HashMap::from([("push".to_string(), json!(push_meta))]),
        read: false,
        created_at: chrono::Local::now().timestamp(),
    };

    if let Err(e) = notification_store.add(notification).await {
        tracing::warn!("保存推送记录失败: {}", e);
    }
}

fn push_report_meta(
    source_event: &str,
    title: &str,
    message: &str,
    level: PushLevel,
    report: &PushDeliveryReport,
) -> HashMap<String, serde_json::Value> {
    let results = &report.results;
    let success_count = results.values().filter(|&&ok| ok).count();
    let failed_count = results.len().saturating_sub(success_count);

    HashMap::from([
        ("source_event".to_string(), json!(source_event)),
        ("push_title".to_string(), json!(title)),
        ("push_message".to_string(), json!(message)),
        ("push_level".to_string(), json!(level.as_str())),
        ("results".to_string(), json!(results)),
        ("errors".to_string(), json!(report.errors)),
        ("attempts".to_string(), json!(report.attempts)),
        ("success_count".to_string(), json!(success_count)),
        ("failed_count".to_string(), json!(failed_count)),
        (
            "channels".to_string(),
            json!(results.keys().cloned().collect::<Vec<_>>()),
        ),
    ])
}

#[cfg(test)]
include!("push/tests.rs");
