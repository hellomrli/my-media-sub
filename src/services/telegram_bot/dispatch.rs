//! `TelegramBotService` 的消息与回调分发。
//!
//! 这两个方法是母文件里最大的两块（`handle_message` 248 行、`handle_callback`
//! 242 行），合起来占原 1197 行 `impl` 块的 41%。提取成同一类型上的第二个
//! `impl` 块后：
//!
//! - 调用点完全不变（方法仍挂在 `TelegramBotService` 上）；
//! - 审查「一条 update 如何被路由」时不必再翻过 500 行无关代码；
//! - 方法体与原来逐字一致，只把可见性放宽到 `pub(super)`。

use super::*;

impl TelegramBotService {
    pub(super) async fn handle_message(&self, update_id: i64, message: TelegramMessage) {
        let started = Instant::now();
        let settings = self.settings_store.get().await;
        let Some(user) = message.from.as_ref() else {
            self.audit_unauthorized(None, message.chat.id, "missing_user")
                .await;
            return;
        };
        if !is_authorized(&settings, user.id, &message.chat) {
            self.audit_unauthorized(Some(user.id), message.chat.id, "not_allowed")
                .await;
            return;
        }
        let Some(raw_text) = message.text.as_deref() else {
            return;
        };
        // 直接粘贴豆瓣链接：解析剧名后进入搜索，等价于 /search <剧名>。
        if let Some(subject_url) = crate::clients::douban::find_subject_url(raw_text) {
            let correlation_id = format!("telegram-update-{update_id}");
            if !self
                .allow_command(user.id, message.chat.id, "douban", false)
                .await
            {
                let response = "操作过于频繁，请稍后再试。";
                let _ = self
                    .send_message(&settings, message.chat.id, response)
                    .await;
                self.record_audit(
                    update_id,
                    None,
                    user.id,
                    message.chat.id,
                    "douban",
                    &subject_url,
                    "rate_limited",
                    response,
                    started.elapsed(),
                    &correlation_id,
                )
                .await;
                return;
            }
            let response = self
                .search_douban_text(&subject_url, user.id, message.chat.id)
                .await;
            let outcome = match self
                .send_text_parts(&settings, message.chat.id, &response)
                .await
            {
                Ok(()) => ("succeeded", response),
                Err(error) => ("failed", error),
            };
            if outcome.0 == "failed" {
                let _ = self
                    .send_message(
                        &settings,
                        message.chat.id,
                        &format!("操作未执行：{}", tg_escape(&outcome.1)),
                    )
                    .await;
            }
            self.record_audit(
                update_id,
                None,
                user.id,
                message.chat.id,
                "douban",
                &subject_url,
                outcome.0,
                &outcome.1,
                started.elapsed(),
                &correlation_id,
            )
            .await;
            if outcome.0 != "failed" {
                self.note_success().await;
            }
            return;
        }
        // 主菜单按钮文案映射为命令
        let mapped = map_menu_text(raw_text);
        let text = mapped.unwrap_or(raw_text);
        if text == "__search_help__" {
            let _ = self
                .send_message_with_markup(
                    &settings,
                    message.chat.id,
                    search_help_text(),
                    Some(main_menu_markup()),
                )
                .await;
            return;
        }
        let Some((command, argument)) = parse_command(text) else {
            return;
        };
        let correlation_id = format!("telegram-update-{update_id}");
        if !self
            .allow_command(user.id, message.chat.id, command, is_write_command(command))
            .await
        {
            let response = "操作过于频繁，请稍后再试。";
            let _ = self
                .send_message(&settings, message.chat.id, response)
                .await;
            self.record_audit(
                update_id,
                None,
                user.id,
                message.chat.id,
                command,
                argument.unwrap_or_default(),
                "rate_limited",
                response,
                started.elapsed(),
                &correlation_id,
            )
            .await;
            return;
        }

        let outcome = if is_write_command(command) {
            let prepared = if command == "switch_apply" {
                self.switch_apply_prepare(argument, user.id, message.chat.id)
                    .await
            } else if command == "subscribe" {
                self.subscribe_prepare(argument, user.id, message.chat.id)
                    .await
            } else if command == "transfer" {
                self.transfer_prepare(argument, user.id, message.chat.id)
                    .await
            } else {
                self.prepare_confirmation(user.id, message.chat.id, command, argument)
                    .await
            };
            match prepared {
                Ok(confirmation) => {
                    let text = confirmation_prompt(&confirmation);
                    let markup = confirmation_markup(&confirmation.nonce);
                    match self
                        .send_message_with_markup(&settings, message.chat.id, &text, Some(markup))
                        .await
                    {
                        Ok(()) => ("confirmation_pending", text),
                        Err(error) => ("failed", error),
                    }
                }
                Err(error) => ("rejected", error),
            }
        } else if command == "menu" || command == "start" {
            let _ = self.ensure_bot_commands(&settings).await;
            let intro = if command == "start" {
                format!("my-media-sub Telegram 控制已连接。\n\n{}", help_text())
            } else {
                "主菜单".to_string()
            };
            match self
                .send_message_with_markup(
                    &settings,
                    message.chat.id,
                    &intro,
                    Some(main_menu_markup()),
                )
                .await
            {
                Ok(()) => ("succeeded", intro),
                Err(error) => ("failed", error),
            }
        } else if command == "search" {
            let response = self
                .search_resources_text(argument.unwrap_or_default(), user.id, message.chat.id)
                .await;
            match self
                .send_text_parts(&settings, message.chat.id, &response)
                .await
            {
                Ok(()) => ("succeeded", response),
                Err(error) => ("failed", error),
            }
        } else if command == "switch" {
            let response = self
                .switch_search_text(argument, user.id, message.chat.id)
                .await;
            match self
                .send_text_parts(&settings, message.chat.id, &response)
                .await
            {
                Ok(()) => ("succeeded", response),
                Err(error) => ("failed", error),
            }
        } else if matches!(
            command,
            "subscriptions" | "jobs" | "notifications" | "calendar"
        ) {
            let page = argument
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            let list = self.list_page(command, page).await;
            let markup = list_page_markup(command, list.page, list.pages);
            match self
                .send_message_with_markup(&settings, message.chat.id, &list.text, markup)
                .await
            {
                Ok(()) => ("succeeded", list.text),
                Err(error) => ("failed", error),
            }
        } else {
            let page = argument
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            let response = self.command_response(command, argument, page).await;
            match self
                .send_text_parts(&settings, message.chat.id, &response)
                .await
            {
                Ok(()) => ("succeeded", response),
                Err(error) => ("failed", error),
            }
        };

        if outcome.0 == "failed" || outcome.0 == "rejected" {
            let _ = self
                .send_message(
                    &settings,
                    message.chat.id,
                    &format!("操作未执行：{}", tg_escape(&outcome.1)),
                )
                .await;
        }
        self.record_audit(
            update_id,
            None,
            user.id,
            message.chat.id,
            command,
            argument.unwrap_or_default(),
            outcome.0,
            &outcome.1,
            started.elapsed(),
            &correlation_id,
        )
        .await;
        if outcome.0 != "failed" {
            self.note_success().await;
        }
    }
    pub(super) async fn handle_callback(&self, update_id: i64, callback: TelegramCallbackQuery) {
        let started = Instant::now();
        match self.telegram_store.claim_callback(&callback.id).await {
            Ok(true) => {}
            Ok(false) => {
                self.diagnostics.lock().await.deduplicated_updates += 1;
                return;
            }
            Err(error) => {
                self.note_error(&error.to_string()).await;
                return;
            }
        }
        let Some(message) = callback.message.as_ref() else {
            // 按钮所属消息过旧/不可用时 Telegram 可省略 message：
            // 仍需应答 callback，避免用户端按钮永远转圈。
            let settings = self.settings_store.get().await;
            let _ = self
                .answer_callback(&settings, &callback.id, "消息已过期，请重新操作", true)
                .await;
            return;
        };
        let settings = self.settings_store.get().await;
        if !is_authorized(&settings, callback.from.id, &message.chat) {
            self.audit_unauthorized(
                Some(callback.from.id),
                message.chat.id,
                "callback_not_allowed",
            )
            .await;
            return;
        }
        let data = callback.data.as_deref().unwrap_or_default();
        if !self
            .allow_command(callback.from.id, message.chat.id, "callback", true)
            .await
        {
            let _ = self
                .answer_callback(&settings, &callback.id, "操作过于频繁，请稍后再试", true)
                .await;
            return;
        }

        if let Some(token) = data.strip_prefix("prompt:") {
            self.handle_prompt_callback(update_id, &callback, message, token, started)
                .await;
            return;
        }

        // 分页按钮：直接编辑当前消息切换只读列表页
        if let Some((command, page)) = parse_page_callback(data) {
            let list = self.list_page(command, page).await;
            let markup = list_page_markup(command, list.page, list.pages);
            let page_note = if list.pages > 1 {
                format!("第 {}/{} 页", list.page, list.pages)
            } else {
                "已到边界".to_string()
            };
            let _ = self
                .answer_callback(&settings, &callback.id, &page_note, false)
                .await;
            if let Some(message_id) = callback.message.as_ref().map(|item| item.message_id) {
                let _ = self
                    .edit_message_with_markup(
                        &settings,
                        message.chat.id,
                        message_id,
                        &list.text,
                        markup,
                    )
                    .await;
            }
            self.record_audit(
                update_id,
                Some(&callback.id),
                callback.from.id,
                message.chat.id,
                &format!("page:{command}"),
                &page.to_string(),
                "succeeded",
                &format!("第 {}/{} 页", list.page, list.pages),
                started.elapsed(),
                &format!("telegram-page-{update_id}"),
            )
            .await;
            self.note_success().await;
            return;
        }

        // 菜单内联：订阅 / 换源序号
        if let Some(index_text) = data.strip_prefix("msub:") {
            let argument = Some(index_text);
            match self
                .subscribe_prepare(argument, callback.from.id, message.chat.id)
                .await
            {
                Ok(confirmation) => {
                    let text = confirmation_prompt(&confirmation);
                    let markup = confirmation_markup(&confirmation.nonce);
                    let _ = self
                        .answer_callback(&settings, &callback.id, "请确认订阅", false)
                        .await;
                    let _ = self
                        .send_message_with_markup(&settings, message.chat.id, &text, Some(markup))
                        .await;
                }
                Err(error) => {
                    let _ = self
                        .answer_callback(&settings, &callback.id, &error, true)
                        .await;
                }
            }
            return;
        }
        if let Some(index_text) = data.strip_prefix("msw:") {
            match self
                .switch_apply_prepare(Some(index_text), callback.from.id, message.chat.id)
                .await
            {
                Ok(confirmation) => {
                    let text = confirmation_prompt(&confirmation);
                    let markup = confirmation_markup(&confirmation.nonce);
                    let _ = self
                        .answer_callback(&settings, &callback.id, "请确认换源", false)
                        .await;
                    let _ = self
                        .send_message_with_markup(&settings, message.chat.id, &text, Some(markup))
                        .await;
                }
                Err(error) => {
                    let _ = self
                        .answer_callback(&settings, &callback.id, &error, true)
                        .await;
                }
            }
            return;
        }
        if let Some(index_text) = data.strip_prefix("mtr:") {
            match self
                .transfer_prepare(Some(index_text), callback.from.id, message.chat.id)
                .await
            {
                Ok(confirmation) => {
                    let text = confirmation_prompt(&confirmation);
                    let markup = confirmation_markup(&confirmation.nonce);
                    let _ = self
                        .answer_callback(&settings, &callback.id, "请确认转存", false)
                        .await;
                    let _ = self
                        .send_message_with_markup(&settings, message.chat.id, &text, Some(markup))
                        .await;
                }
                Err(error) => {
                    let _ = self
                        .answer_callback(&settings, &callback.id, &error, true)
                        .await;
                }
            }
            return;
        }

        let (decision, nonce) = data.split_once(':').unwrap_or(("", ""));
        if !matches!(decision, "confirm" | "cancel") || nonce.is_empty() {
            let _ = self
                .answer_callback(&settings, &callback.id, "无效操作", true)
                .await;
            return;
        }
        let confirmation = self
            .claim_confirmation(
                nonce,
                callback.from.id,
                message.chat.id,
                decision == "confirm",
            )
            .await;
        let confirmation = match confirmation {
            Ok(value) => value,
            Err(error) => {
                let _ = self
                    .answer_callback(&settings, &callback.id, &error, true)
                    .await;
                return;
            }
        };
        if decision == "cancel" {
            let _ = self
                .answer_callback(&settings, &callback.id, "操作已取消", false)
                .await;
            let _ = self
                .send_message(&settings, message.chat.id, "操作已取消。")
                .await;
            return;
        }
        let Some(confirmation) = confirmation else {
            return;
        };
        let _ = self
            .answer_callback(&settings, &callback.id, "已确认，正在执行", false)
            .await;
        let correlation_id = format!("telegram-action-{}", confirmation.nonce);
        let result = if self
            .telegram_store
            .claim_action(&confirmation.idempotency_key)
            .await
            .unwrap_or(false)
        {
            self.execute_confirmation(&confirmation, &correlation_id)
                .await
        } else {
            Err("该操作已经执行或正在执行".to_string())
        };
        let (outcome, response) = match result {
            Ok(response) => {
                self.record_action_outcome(callback.from.id, message.chat.id, true)
                    .await;
                ("succeeded", response)
            }
            Err(error) => {
                self.record_action_outcome(callback.from.id, message.chat.id, false)
                    .await;
                ("failed", format!("操作失败：{}", tg_escape(&error)))
            }
        };
        let _ = self
            .send_text_parts(&settings, message.chat.id, &response)
            .await;
        self.record_audit(
            update_id,
            Some(&callback.id),
            callback.from.id,
            message.chat.id,
            &confirmation.action,
            &confirmation.resource,
            outcome,
            &response,
            started.elapsed(),
            &correlation_id,
        )
        .await;
        self.note_success().await;
    }
}
