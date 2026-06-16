use crate::error::{AppError, Result};
use crate::models::Settings;
use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

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
    fn as_str(&self) -> &str {
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

    /// 发送推送到所有启用的渠道
    #[allow(dead_code)]
    pub async fn send(
        &self,
        title: &str,
        message: &str,
        level: PushLevel,
    ) -> HashMap<String, bool> {
        let channels = self.enabled_channels();
        self.send_to_channels(&channels, title, message, level)
            .await
    }

    /// 发送推送到指定渠道
    pub async fn send_to_channels(
        &self,
        channels: &[String],
        title: &str,
        message: &str,
        level: PushLevel,
    ) -> HashMap<String, bool> {
        let enabled_channels = self.enabled_channels();
        let mut results = HashMap::new();
        for channel in channels {
            if !enabled_channels.contains(channel) {
                continue;
            }

            let result = match channel.as_str() {
                "wecom" => self.send_wecom(title, message, level).await,
                "wxpusher" => self.send_wxpusher(title, message).await,
                "telegram" => self.send_telegram(title, message, level, false).await,
                "bark" => self.send_bark(title, message, level).await,
                "gotify" => self.send_gotify(title, message, level).await,
                "pushplus" => self.send_pushplus(title, message).await,
                "serverchan" => self.send_serverchan(title, message).await,
                _ => Ok(false),
            };

            results.insert(channel.clone(), result.unwrap_or(false));
        }

        results
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_level() {
        assert_eq!(PushLevel::Info.as_str(), "info");
        assert_eq!(PushLevel::Success.emoji(), "✅");
        assert_eq!(PushLevel::Warning.emoji(), "⚠️");
        assert_eq!(PushLevel::Error.emoji(), "❌");
    }

    #[test]
    fn test_enabled_channels() {
        let mut settings = Settings::default();
        settings.wecom_bot_url = "https://test".to_string();
        settings.telegram_bot_token = "token".to_string();
        settings.telegram_chat_id = "123".to_string();

        let service = PushService::new(settings);
        let channels = service.enabled_channels();

        assert_eq!(channels.len(), 2);
        assert!(channels.contains(&"wecom".to_string()));
        assert!(channels.contains(&"telegram".to_string()));
    }
}
