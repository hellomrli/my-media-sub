macro_rules! push_channel_methods {
    () => {
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
            .send_observed("push")
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
            .send_observed("push")
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
        let mut payload = json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML",
            "disable_notification": silent,
        });
        if let Some(reply_markup) = &self.telegram_reply_markup {
            payload["reply_markup"] = reply_markup.clone();
        }

        let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send_observed("push")
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
            .send_observed("push")
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
            .send_observed("push")
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
            .send_observed("push")
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
            .send_observed("push")
            .await
            .map_err(|e| AppError::Http(format!("Server酱推送失败: {}", e)))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;
        Ok(data.get("code").and_then(|v| v.as_i64()) == Some(0))
    }
    };
}
