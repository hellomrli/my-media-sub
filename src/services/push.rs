use crate::error::{AppError, Result};
use crate::models::{Notification, Settings};
use crate::store::NotificationStore;
use regex::Regex;
use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

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
    pub fn from_str(value: &str) -> Option<Self> {
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
}

impl PushEvent {
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "subscription_updated" => Some(Self::SubscriptionUpdated),
            "subscription_failed" => Some(Self::SubscriptionFailed),
            "subscription_completed" => Some(Self::SubscriptionCompleted),
            "transfer_saved" => Some(Self::TransferSaved),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::SubscriptionUpdated => "subscription_updated",
            Self::SubscriptionFailed => "subscription_failed",
            Self::SubscriptionCompleted => "subscription_completed",
            Self::TransferSaved => "transfer_saved",
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

/// 推送服务
pub struct PushService {
    settings: Settings,
    client: Client,
}

impl PushService {
    pub fn new(settings: Settings) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();

        Self { settings, client }
    }

    /// 获取已启用的推送渠道
    pub fn enabled_channels(&self) -> Vec<String> {
        let mut channels = Vec::new();

        if !self.settings.wecom_bot_url.is_empty() {
            channels.push("wecom".to_string());
        }
        if !self.settings.wxpusher_app_token.is_empty() {
            channels.push("wxpusher".to_string());
        }
        if !self.settings.telegram_bot_token.is_empty()
            && !self.settings.telegram_chat_id.is_empty()
        {
            channels.push("telegram".to_string());
        }
        if !self.settings.bark_url.is_empty() {
            channels.push("bark".to_string());
        }
        if !self.settings.gotify_url.is_empty() && !self.settings.gotify_token.is_empty() {
            channels.push("gotify".to_string());
        }
        if !self.settings.pushplus_token.is_empty() {
            channels.push("pushplus".to_string());
        }
        if !self.settings.serverchan_key.is_empty() {
            channels.push("serverchan".to_string());
        }

        channels
    }

    pub fn event_enabled(&self, event: PushEvent) -> bool {
        match event {
            PushEvent::SubscriptionUpdated => self.settings.push_on_update,
            PushEvent::SubscriptionFailed => self.settings.push_on_failed,
            PushEvent::SubscriptionCompleted => self.settings.push_on_completed,
            PushEvent::TransferSaved => self.settings.push_on_save,
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
        let enabled_channels = self.enabled_channels();
        let mut report = PushDeliveryReport::default();
        for channel in channels {
            if !enabled_channels.contains(channel) {
                report.results.insert(channel.clone(), false);
                report
                    .errors
                    .insert(channel.clone(), "渠道未配置或未启用".to_string());
                report.attempts.insert(channel.clone(), 0);
                continue;
            }

            let (success, attempts, last_error) = send_with_retry(retry_policy, || {
                self.send_channel(channel, title, message, level)
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

    async fn send_channel(
        &self,
        channel: &str,
        title: &str,
        message: &str,
        level: PushLevel,
    ) -> Result<bool> {
        match channel {
            "wecom" => self.send_wecom(title, message, level).await,
            "wxpusher" => self.send_wxpusher(title, message).await,
            "telegram" => {
                self.send_telegram(title, message, level, self.settings.push_silent)
                    .await
            }
            "bark" => self.send_bark(title, message, level).await,
            "gotify" => self.send_gotify(title, message, level).await,
            "pushplus" => self.send_pushplus(title, message).await,
            "serverchan" => self.send_serverchan(title, message).await,
            _ => Ok(false),
        }
    }

    /// 企业微信机器人
    async fn send_wecom(&self, title: &str, message: &str, level: PushLevel) -> Result<bool> {
        let url = &self.settings.wecom_bot_url;
        if url.is_empty() {
            return Ok(false);
        }

        let now = chrono::Local::now().format("%m-%d %H:%M").to_string();
        let content = format!("### {} {}\n{}\n> {}", level.emoji(), title, message, now);

        let payload = json!({
            "msgtype": "markdown",
            "markdown": {
                "content": content,
            },
        });

        let resp = self
            .client
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("企业微信推送失败: {}", e)))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;
        Ok(data.get("errcode").and_then(|v| v.as_i64()) == Some(0))
    }

    /// WxPusher
    async fn send_wxpusher(&self, title: &str, message: &str) -> Result<bool> {
        let token = &self.settings.wxpusher_app_token;
        if token.is_empty() {
            return Ok(false);
        }

        let uids: Vec<String> = if !self.settings.wxpusher_uids.is_empty() {
            self.settings
                .wxpusher_uids
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            vec![]
        };

        let payload = json!({
            "appToken": token,
            "content": format!("<h3>{}</h3><p>{}</p>", title, message),
            "summary": title,
            "contentType": 2,
            "uids": uids,
        });

        let resp = self
            .client
            .post("https://wxpusher.zjiecode.com/api/send/message")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("WxPusher 推送失败: {}", e)))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;
        Ok(data.get("code").and_then(|v| v.as_i64()) == Some(1000))
    }

    /// Telegram Bot
    async fn send_telegram(
        &self,
        title: &str,
        message: &str,
        level: PushLevel,
        silent: bool,
    ) -> Result<bool> {
        let token = &self.settings.telegram_bot_token;
        let chat_id = &self.settings.telegram_chat_id;

        if token.is_empty() || chat_id.is_empty() {
            return Ok(false);
        }

        let text = format!("{} <b>{}</b>\n\n{}", level.emoji(), title, message);
        let payload = json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML",
            "disable_notification": silent,
        });

        let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("Telegram 推送失败: {}", e)))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;
        Ok(data.get("ok").and_then(|v| v.as_bool()).unwrap_or(false))
    }

    /// Bark (iOS)
    async fn send_bark(&self, title: &str, message: &str, level: PushLevel) -> Result<bool> {
        let url = self.settings.bark_url.trim_end_matches('/');
        if url.is_empty() {
            return Ok(false);
        }

        let payload = json!({
            "title": title,
            "body": message,
            "level": level.as_str(),
            "badge": 1,
        });

        let resp = self
            .client
            .post(format!("{}/push", url))
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("Bark 推送失败: {}", e)))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;
        Ok(data.get("code").and_then(|v| v.as_i64()) == Some(200))
    }

    /// Gotify
    async fn send_gotify(&self, title: &str, message: &str, level: PushLevel) -> Result<bool> {
        let url = self.settings.gotify_url.trim_end_matches('/');
        let token = &self.settings.gotify_token;

        if url.is_empty() || token.is_empty() {
            return Ok(false);
        }

        let priority = match level {
            PushLevel::Info | PushLevel::Success => 5,
            PushLevel::Warning => 7,
            PushLevel::Error => 9,
        };

        let payload = json!({
            "title": title,
            "message": message,
            "priority": priority,
        });

        let resp = self
            .client
            .post(format!("{}/message?token={}", url, token))
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("Gotify 推送失败: {}", e)))?;

        Ok(resp.status().is_success())
    }

    /// PushPlus
    async fn send_pushplus(&self, title: &str, message: &str) -> Result<bool> {
        let token = &self.settings.pushplus_token;
        if token.is_empty() {
            return Ok(false);
        }

        let payload = json!({
            "token": token,
            "title": title,
            "content": message,
            "template": "html",
        });

        let resp = self
            .client
            .post("http://www.pushplus.plus/send")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("PushPlus 推送失败: {}", e)))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;
        Ok(data.get("code").and_then(|v| v.as_i64()) == Some(200))
    }

    /// Server酱
    async fn send_serverchan(&self, title: &str, message: &str) -> Result<bool> {
        let key = &self.settings.serverchan_key;
        if key.is_empty() {
            return Ok(false);
        }

        let payload = json!({
            "title": title,
            "desp": message,
        });

        let url = format!("https://sctapi.ftqq.com/{}.send", key);
        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("Server酱推送失败: {}", e)))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;
        Ok(data.get("code").and_then(|v| v.as_i64()) == Some(0))
    }
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
    let token_re = Regex::new(r"(?i)(token|key|sendkey|access_token)=([^&\s]+)").unwrap();
    let bot_re = Regex::new(r"(?i)bot[0-9]+:[A-Za-z0-9_-]+").unwrap();
    let serverchan_re = Regex::new(r"SCT[A-Za-z0-9]+").unwrap();

    let sanitized = token_re.replace_all(value, "$1=***");
    let sanitized = bot_re.replace_all(&sanitized, "bot***");
    let sanitized = serverchan_re.replace_all(&sanitized, "SCT***");
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
    let results = &report.results;
    let errors = &report.errors;

    if results.is_empty() {
        return;
    }

    let success_count = results.values().filter(|&&ok| ok).count();
    let failed_count = results.len().saturating_sub(success_count);
    let record_level = if failed_count > 0 {
        "warning"
    } else {
        level.as_str()
    };

    let notification = Notification {
        id: uuid::Uuid::new_v4().to_string(),
        level: record_level.to_string(),
        event: "push_sent".to_string(),
        title: format!("推送记录: {}", title),
        message: message.to_string(),
        meta: HashMap::from([
            ("source_event".to_string(), json!(source_event)),
            ("push_title".to_string(), json!(title)),
            ("push_message".to_string(), json!(message)),
            ("push_level".to_string(), json!(level.as_str())),
            ("results".to_string(), json!(results)),
            ("errors".to_string(), json!(errors)),
            ("attempts".to_string(), json!(report.attempts)),
            ("success_count".to_string(), json!(success_count)),
            ("failed_count".to_string(), json!(failed_count)),
            (
                "channels".to_string(),
                json!(results.keys().cloned().collect::<Vec<_>>()),
            ),
        ]),
        read: false,
        created_at: chrono::Local::now().timestamp(),
    };

    if let Err(e) = notification_store.add(notification).await {
        tracing::warn!("保存推送记录失败: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NotificationStore;

    #[test]
    fn test_push_level() {
        assert_eq!(PushLevel::Info.as_str(), "info");
        assert_eq!(PushLevel::Success.emoji(), "✅");
        assert_eq!(PushLevel::Warning.emoji(), "⚠️");
        assert_eq!(PushLevel::Error.emoji(), "❌");
    }

    #[test]
    fn test_enabled_channels() {
        let settings = Settings {
            wecom_bot_url: "https://test".to_string(),
            telegram_bot_token: "token".to_string(),
            telegram_chat_id: "123".to_string(),
            ..Default::default()
        };

        let service = PushService::new(settings);
        let channels = service.enabled_channels();

        assert_eq!(channels.len(), 2);
        assert!(channels.contains(&"wecom".to_string()));
        assert!(channels.contains(&"telegram".to_string()));
    }

    #[test]
    fn test_retry_policy_uses_exponential_backoff_with_cap() {
        let policy = PushRetryPolicy {
            max_attempts: 0,
            initial_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(5),
        };

        assert_eq!(policy.attempts(), 1);
        assert_eq!(policy.delay_for_retry(0), Duration::from_secs(2));
        assert_eq!(policy.delay_for_retry(1), Duration::from_secs(4));
        assert_eq!(policy.delay_for_retry(2), Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_send_to_channels_retries_until_success() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_send = attempts.clone();
        let (success, attempt_count, last_error) = send_with_retry(
            PushRetryPolicy {
                max_attempts: 3,
                initial_delay: Duration::ZERO,
                max_delay: Duration::ZERO,
            },
            move || {
                let attempts_for_send = attempts_for_send.clone();
                async move {
                    let attempt = attempts_for_send.fetch_add(1, Ordering::SeqCst) + 1;
                    Ok::<bool, AppError>(attempt == 3)
                }
            },
        )
        .await;

        assert!(success);
        assert_eq!(attempt_count, 3);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert_eq!(last_error, "渠道返回失败状态");
    }

    #[test]
    fn test_sanitize_push_error_masks_tokens() {
        let error = "request failed: https://example.com/message?token=abc123&key=secret bot123456:ABC_def SCTabcdef";
        let sanitized = sanitize_push_error(error);

        assert!(sanitized.contains("token=***"));
        assert!(sanitized.contains("key=***"));
        assert!(sanitized.contains("bot***"));
        assert!(sanitized.contains("SCT***"));
        assert!(!sanitized.contains("abc123"));
        assert!(!sanitized.contains("secret"));
        assert!(!sanitized.contains("ABC_def"));
    }

    #[tokio::test]
    async fn test_send_event_respects_global_switch() {
        let settings = Settings {
            push_on_update: false,
            wecom_bot_url: "https://test".to_string(),
            ..Default::default()
        };

        let service = PushService::new(settings);
        let results = service
            .send_event(
                PushEvent::SubscriptionUpdated,
                "title",
                "message",
                PushLevel::Info,
            )
            .await;

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_record_push_message_saves_results() {
        let tmp =
            std::env::temp_dir().join(format!("my-media-sub-push-{}.json", uuid::Uuid::new_v4()));
        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();

        let results = HashMap::from([("telegram".to_string(), true), ("bark".to_string(), false)]);
        record_push_message(
            &store,
            PushEvent::SubscriptionUpdated.as_str(),
            "title",
            "message",
            PushLevel::Info,
            &results,
        )
        .await;

        let notifications = store.list(true).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].event, "push_sent");
        assert_eq!(notifications[0].level, "warning");
        assert_eq!(notifications[0].meta["success_count"], json!(1));
        assert_eq!(notifications[0].meta["failed_count"], json!(1));

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn test_record_push_message_saves_attempts() {
        let tmp = std::env::temp_dir().join(format!(
            "my-media-sub-push-attempts-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();

        let report = PushDeliveryReport {
            results: HashMap::from([("telegram".to_string(), false)]),
            errors: HashMap::from([("telegram".to_string(), "尝试 3 次后失败".to_string())]),
            attempts: HashMap::from([("telegram".to_string(), 3)]),
        };

        record_push_message_report(
            &store,
            "subscription_updated",
            "title",
            "message",
            PushLevel::Info,
            &report,
        )
        .await;

        let notifications = store.list(true).await;
        assert_eq!(notifications[0].meta["attempts"]["telegram"], json!(3));
        assert_eq!(
            notifications[0].meta["errors"]["telegram"],
            json!("尝试 3 次后失败")
        );

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn test_record_push_message_saves_sanitized_errors() {
        let tmp = std::env::temp_dir().join(format!(
            "my-media-sub-push-errors-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();

        let results = HashMap::from([("gotify".to_string(), false)]);
        let errors = HashMap::from([(
            "gotify".to_string(),
            sanitize_push_error("https://gotify.example/message?token=secret-token failed"),
        )]);
        record_push_message_with_errors(
            &store,
            "push_test",
            "title",
            "message",
            PushLevel::Info,
            &results,
            &errors,
        )
        .await;

        let notifications = store.list(true).await;
        assert_eq!(
            notifications[0].meta["errors"]["gotify"],
            json!("https://gotify.example/message?token=*** failed")
        );

        let _ = std::fs::remove_file(tmp);
    }
}
