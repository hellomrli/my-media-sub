impl TelegramBotService {
    async fn prepare_confirmation(
        &self,
        user_id: i64,
        chat_id: i64,
        command: &str,
        argument: Option<&str>,
    ) -> Result<PendingConfirmation, String> {
        let resource = match command {
            "check" => {
                let argument = argument.ok_or_else(|| "用法：/check <订阅ID|all>".to_string())?;
                if argument == "all" {
                    "all".to_string()
                } else {
                    self.resolve_subscription(argument).await?
                }
            }
            "retry" | "cancel" => {
                let argument = argument.ok_or_else(|| format!("用法：/{command} <Job ID>"))?;
                self.resolve_job(argument).await?
            }
            "read" => {
                let argument = argument.ok_or_else(|| "用法：/read <通知ID|all>".to_string())?;
                if argument == "all" {
                    "all".to_string()
                } else {
                    let notifications = self.notification_store.list(true).await;
                    resolve_unique_id(
                        notifications.iter().map(|item| item.id.as_str()),
                        argument,
                        "通知",
                    )?
                }
            }
            "signin" => "quark".to_string(),
            _ => return Err("该命令不是允许的写操作".to_string()),
        };
        let scope = bot_action_scope(command, &resource)?;
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let confirmation = PendingConfirmation {
            nonce: nonce.clone(),
            user_id,
            chat_id,
            action: command.to_string(),
            scope: scope.to_string(),
            resource: resource.clone(),
            expires_at: crate::utils::unix_now() + CONFIRMATION_TTL_SECONDS,
            idempotency_key: format!(
                "telegram:{scope}:{user_id}:{chat_id}:{command}:{resource}:{nonce}"
            ),
        };
        let mut confirmations = self.confirmations.lock().await;
        let now = crate::utils::unix_now();
        confirmations.retain(|_, item| item.expires_at >= now);
        if confirmations.len() >= 1_000 {
            return Err("待确认操作过多，请稍后再试".to_string());
        }
        confirmations.insert(nonce, confirmation.clone());
        Ok(confirmation)
    }

    async fn claim_confirmation(
        &self,
        nonce: &str,
        user_id: i64,
        chat_id: i64,
        execute: bool,
    ) -> Result<Option<PendingConfirmation>, String> {
        let mut confirmations = self.confirmations.lock().await;
        let Some(item) = confirmations.get(nonce) else {
            return Err("确认已使用、已失效或服务已重启".to_string());
        };
        if item.user_id != user_id || item.chat_id != chat_id {
            return Err("确认不属于当前用户或会话".to_string());
        }
        if item.expires_at < crate::utils::unix_now() {
            confirmations.remove(nonce);
            return Err("确认已过期，请重新发起命令".to_string());
        }
        let item = confirmations
            .remove(nonce)
            .expect("confirmation was checked while lock is held");
        Ok(execute.then_some(item))
    }

    async fn execute_confirmation(
        &self,
        confirmation: &PendingConfirmation,
        correlation_id: &str,
    ) -> Result<String, String> {
        let context = crate::observability::LogContext {
            request_id: Some(format!("telegram-{}", confirmation.nonce)),
            correlation_id: Some(correlation_id.to_string()),
            subscription_id: (confirmation.action == "check" && confirmation.resource != "all")
                .then(|| confirmation.resource.clone()),
            job_id: matches!(confirmation.action.as_str(), "retry" | "cancel")
                .then(|| confirmation.resource.clone()),
        };
        let span = tracing::info_span!(
            "telegram_action",
            action = %confirmation.action,
            resource = %confirmation.resource,
            correlation_id = %correlation_id
        );
        crate::observability::in_context(context, span, async {
            match confirmation.action.as_str() {
                "check" => {
                    let settings = self.settings_store.get().await;
                    if settings.quark_cookie.trim().is_empty() {
                        return Err("未配置夸克 Cookie".to_string());
                    }
                    if confirmation.resource == "all" {
                        let results = self
                            .check_service
                            .check_all_subscriptions(&settings.quark_cookie)
                            .await
                            .map_err(|error| sanitize_error(&error.to_string()))?;
                        Ok(format!(
                            "全部订阅检查完成：{} 个结果\nrequest: telegram-{}\ncorrelation: {}",
                            results.len(),
                            confirmation.nonce,
                            correlation_id
                        ))
                    } else {
                        let result = self
                            .check_service
                            .check_subscription(&confirmation.resource, &settings.quark_cookie)
                            .await
                            .map_err(|error| sanitize_error(&error.to_string()))?;
                        Ok(format!(
                            "订阅检查完成：{}\n{}\nrequest: telegram-{}\ncorrelation: {}",
                            result.subscription_title,
                            result.summary,
                            confirmation.nonce,
                            correlation_id
                        ))
                    }
                }
                "retry" => {
                    let job = self
                        .job_queue
                        .retry(&confirmation.resource)
                        .await
                        .map_err(|error| sanitize_error(&error.to_string()))?;
                    Ok(format!(
                        "任务已重新入队\nrequest: telegram-{}\njob: {}\ncorrelation: {}",
                        confirmation.nonce,
                        job.id,
                        job.correlation_id.as_deref().unwrap_or(correlation_id)
                    ))
                }
                "cancel" => {
                    let job = self
                        .job_queue
                        .cancel(&confirmation.resource)
                        .await
                        .map_err(|error| sanitize_error(&error.to_string()))?;
                    Ok(format!(
                        "任务已取消\nrequest: telegram-{}\njob: {}\ncorrelation: {}",
                        confirmation.nonce, job.id, correlation_id
                    ))
                }
                "signin" => {
                    let result = self
                        .signin_service
                        .signin_with_failure_notice()
                        .await
                        .map_err(|error| sanitize_error(&error.to_string()))?;
                    Ok(format!(
                        "夸克签到完成：{}\nrequest: telegram-{}\ncorrelation: {}",
                        if result.already_signed {
                            "今日已签到"
                        } else if result.signed {
                            "签到成功"
                        } else {
                            "未获得签到结果"
                        },
                        confirmation.nonce,
                        correlation_id
                    ))
                }
                "read" => {
                    self.notification_store
                        .mark_read(
                            (confirmation.resource != "all")
                                .then_some(confirmation.resource.as_str()),
                        )
                        .await
                        .map_err(|error| sanitize_error(&error.to_string()))?;
                    Ok(format!(
                        "通知已标记为已读：{}\nrequest: telegram-{}\ncorrelation: {}",
                        confirmation.resource, confirmation.nonce, correlation_id
                    ))
                }
                _ => Err("不允许的操作".to_string()),
            }
        })
        .await
    }

    async fn resolve_subscription(&self, value: &str) -> Result<String, String> {
        let items = self.subscription_store.list().await;
        resolve_unique_id(items.iter().map(|item| item.id.as_str()), value, "订阅")
    }

    async fn resolve_job(&self, value: &str) -> Result<String, String> {
        let items = self.job_store.list().await;
        resolve_unique_id(items.iter().map(|item| item.id.as_str()), value, "任务")
    }

    async fn allow_command(&self, user_id: i64, chat_id: i64, command: &str, write: bool) -> bool {
        let now = crate::utils::unix_now();
        let mut rates = self.command_rates.lock().await;
        if rates
            .failures
            .get(&format!("{user_id}:{chat_id}"))
            .is_some_and(|(_, cooldown)| *cooldown > now)
        {
            self.diagnostics.lock().await.rate_limited_updates += 1;
            return false;
        }
        let checks = [
            (format!("user:{user_id}"), 20_usize),
            (format!("chat:{chat_id}"), 30_usize),
            (
                format!("command:{user_id}:{chat_id}:{command}"),
                if write { 6 } else { 15 },
            ),
        ];
        // 全局清理：整个窗口内没有新尝试的键直接移除，
        // 避免 attempts map 随用户/命令组合无界增长。
        rates.attempts.retain(|_, attempts| {
            attempts
                .back()
                .is_some_and(|at| now.saturating_sub(*at) < RATE_WINDOW_SECONDS)
        });
        let mut allowed = true;
        for (key, limit) in &checks {
            let mut remove_key = false;
            let count = match rates.attempts.get_mut(key) {
                Some(attempts) => {
                    while attempts
                        .front()
                        .is_some_and(|at| now.saturating_sub(*at) >= RATE_WINDOW_SECONDS)
                    {
                        attempts.pop_front();
                    }
                    remove_key = attempts.is_empty();
                    attempts.len()
                }
                None => 0,
            };
            if remove_key {
                rates.attempts.remove(key);
            }
            if count >= *limit {
                allowed = false;
            }
        }
        if allowed {
            for (key, _) in checks {
                rates.attempts.entry(key).or_default().push_back(now);
            }
        } else {
            self.diagnostics.lock().await.rate_limited_updates += 1;
        }
        allowed
    }

    async fn record_action_outcome(&self, user_id: i64, chat_id: i64, succeeded: bool) {
        let key = format!("{user_id}:{chat_id}");
        let mut rates = self.command_rates.lock().await;
        if succeeded {
            rates.failures.remove(&key);
            return;
        }
        let entry = rates.failures.entry(key).or_insert((0, 0));
        entry.0 = entry.0.saturating_add(1);
        if entry.0 >= 3 {
            entry.1 = crate::utils::unix_now() + FAILURE_COOLDOWN_SECONDS;
            entry.0 = 0;
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_audit(
        &self,
        update_id: i64,
        callback_id: Option<&str>,
        user_id: i64,
        chat_id: i64,
        command: &str,
        target: &str,
        result: &str,
        message: &str,
        duration: Duration,
        correlation_id: &str,
    ) {
        let settings = self.settings_store.get().await;
        let audit = TelegramCommandAudit {
            id: uuid::Uuid::new_v4().to_string(),
            update_id,
            callback_id: callback_id.map(ToString::to_string),
            user_id,
            chat_id,
            command: one_line(command, 64),
            target: one_line(target, 128),
            result: result.to_string(),
            message: sanitize_error_with_settings(message, &settings),
            duration_ms: duration.as_millis().min(u128::from(u64::MAX)) as u64,
            correlation_id: correlation_id.to_string(),
            created_at: crate::utils::unix_now(),
        };
        if let Err(error) = self.telegram_store.add_audit(audit).await {
            tracing::warn!("记录 Telegram 命令审计失败: {error}");
        }
    }

    async fn send_text_parts(
        &self,
        settings: &Settings,
        chat_id: i64,
        text: &str,
    ) -> Result<(), String> {
        for part in split_message(text)
            .into_iter()
            .take(MAX_MESSAGES_PER_COMMAND)
        {
            self.send_message(settings, chat_id, &part).await?;
        }
        Ok(())
    }

    async fn handle_prompt_callback(
        &self,
        update_id: i64,
        callback: &TelegramCallbackQuery,
        message: &TelegramMessage,
        token: &str,
        started: Instant,
    ) {
        let settings = self.settings_store.get().await;
        let verified =
            verify_prompt_callback_data(&settings, token, callback.from.id, message.chat.id);
        let (action, resource) = match verified {
            Ok(value) => value,
            Err(error) => {
                let _ = self
                    .answer_callback(&settings, &callback.id, &error, true)
                    .await;
                return;
            }
        };
        if action == "view" {
            let notifications = self.notification_store.list(true).await;
            let response = notifications
                .iter()
                .find(|item| item.id == resource)
                .map(|item| format!("{}\n{}\n事件：{}", item.title, item.message, item.event))
                .unwrap_or_else(|| "通知不存在或已清理".to_string());
            let _ = self
                .answer_callback(&settings, &callback.id, "详情已发送", false)
                .await;
            let _ = self
                .send_text_parts(&settings, message.chat.id, &response)
                .await;
            self.record_audit(
                update_id,
                Some(&callback.id),
                callback.from.id,
                message.chat.id,
                "view",
                &resource,
                "succeeded",
                &response,
                started.elapsed(),
                &format!("telegram-prompt-{update_id}"),
            )
            .await;
            return;
        }
        let confirmation = match self
            .prepare_confirmation(callback.from.id, message.chat.id, &action, Some(&resource))
            .await
        {
            Ok(value) => value,
            Err(error) => {
                let _ = self
                    .answer_callback(&settings, &callback.id, &error, true)
                    .await;
                return;
            }
        };
        let prompt = confirmation_prompt(&confirmation);
        let markup = confirmation_markup(&confirmation.nonce);
        let _ = self
            .answer_callback(&settings, &callback.id, "请再次确认操作", false)
            .await;
        let _ = self
            .send_message_with_markup(&settings, message.chat.id, &prompt, Some(markup))
            .await;
        self.record_audit(
            update_id,
            Some(&callback.id),
            callback.from.id,
            message.chat.id,
            &action,
            &resource,
            "confirmation_pending",
            &prompt,
            started.elapsed(),
            &format!("telegram-prompt-{update_id}"),
        )
        .await;
    }

}
