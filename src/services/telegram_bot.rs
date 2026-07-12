use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{Mutex, Semaphore};

use crate::jobs::{JobQueue, JobStatus, JobStore};
use crate::models::Settings;
use crate::services::media_calendar::{
    build_media_calendar, natural_week, shanghai_offset, MediaCalendarQuery,
};
use crate::services::{QuarkSigninService, SubscriptionCheckService};
use crate::store::{
    AutomationEventStore, NotificationStore, SettingsStore, SubscriptionStore, TelegramBotStore,
    TelegramCommandAudit,
};

const TELEGRAM_API_BASE: &str = "https://api.telegram.org";
const TELEGRAM_MESSAGE_LIMIT: usize = 3_500;
const MAX_MESSAGES_PER_COMMAND: usize = 4;
const LIST_PAGE_SIZE: usize = 8;
const SECURITY_AUDIT_INTERVAL_SECONDS: i64 = 60;
const CONFIRMATION_TTL_SECONDS: i64 = 120;
const RATE_WINDOW_SECONDS: i64 = 60;
const FAILURE_COOLDOWN_SECONDS: i64 = 60;
/// 单个进程内并发处理的 Telegram Update 上限。慢命令（如 /check all）
/// 在独立任务中执行，不阻塞 getUpdates 长轮询循环。
const MAX_CONCURRENT_UPDATES: usize = 8;

#[derive(Debug, Clone, Serialize, Default)]
pub struct TelegramBotDiagnostics {
    pub mode: String,
    pub status: String,
    pub last_update_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub last_error: Option<String>,
    pub unauthorized_updates: u64,
    pub deduplicated_updates: u64,
    pub rate_limited_updates: u64,
    pub audit_count: usize,
    pub pending_confirmations: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramUpdate {
    pub update_id: i64,
    #[serde(default)]
    pub message: Option<TelegramMessage>,
    #[serde(default)]
    pub callback_query: Option<TelegramCallbackQuery>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramCallbackQuery {
    pub id: String,
    pub from: TelegramUser,
    #[serde(default)]
    pub message: Option<TelegramMessage>,
    #[serde(default)]
    pub data: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramMessage {
    pub message_id: i64,
    pub chat: TelegramChat,
    #[serde(default)]
    pub from: Option<TelegramUser>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramChat {
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramUser {
    pub id: i64,
}

#[derive(Debug, Deserialize)]
struct TelegramApiResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramOutboundMessage {
    pub chat_id: i64,
    pub text: String,
}

#[derive(Debug, Clone)]
struct PendingConfirmation {
    nonce: String,
    user_id: i64,
    chat_id: i64,
    action: String,
    scope: String,
    resource: String,
    expires_at: i64,
    idempotency_key: String,
}

#[derive(Debug, Default)]
struct CommandRateState {
    attempts: HashMap<String, VecDeque<i64>>,
    failures: HashMap<String, (u32, i64)>,
}

pub struct TelegramBotDependencies {
    pub settings_store: Arc<SettingsStore>,
    pub subscription_store: Arc<SubscriptionStore>,
    pub notification_store: Arc<NotificationStore>,
    pub automation_event_store: Arc<AutomationEventStore>,
    pub job_store: Arc<JobStore>,
    pub job_queue: Arc<JobQueue>,
    pub check_service: Arc<SubscriptionCheckService>,
    pub signin_service: Arc<QuarkSigninService>,
    pub telegram_store: Arc<TelegramBotStore>,
}

pub struct TelegramBotService {
    settings_store: Arc<SettingsStore>,
    subscription_store: Arc<SubscriptionStore>,
    notification_store: Arc<NotificationStore>,
    automation_event_store: Arc<AutomationEventStore>,
    job_store: Arc<JobStore>,
    job_queue: Arc<JobQueue>,
    check_service: Arc<SubscriptionCheckService>,
    signin_service: Arc<QuarkSigninService>,
    telegram_store: Arc<TelegramBotStore>,
    client: Client,
    api_base: String,
    diagnostics: Mutex<TelegramBotDiagnostics>,
    security_audits: Mutex<HashMap<String, i64>>,
    confirmations: Mutex<HashMap<String, PendingConfirmation>>,
    command_rates: Mutex<CommandRateState>,
}

impl TelegramBotService {
    pub fn new(dependencies: TelegramBotDependencies) -> Self {
        Self::with_api_base(dependencies, TELEGRAM_API_BASE)
    }

    fn with_api_base(dependencies: TelegramBotDependencies, api_base: &str) -> Self {
        Self {
            settings_store: dependencies.settings_store,
            subscription_store: dependencies.subscription_store,
            notification_store: dependencies.notification_store,
            automation_event_store: dependencies.automation_event_store,
            job_store: dependencies.job_store,
            job_queue: dependencies.job_queue,
            check_service: dependencies.check_service,
            signin_service: dependencies.signin_service,
            telegram_store: dependencies.telegram_store,
            client: Client::builder()
                .timeout(Duration::from_secs(35))
                .build()
                .unwrap_or_else(|_| Client::new()),
            api_base: api_base.trim_end_matches('/').to_string(),
            diagnostics: Mutex::new(TelegramBotDiagnostics {
                mode: "disabled".to_string(),
                status: "disabled".to_string(),
                ..Default::default()
            }),
            security_audits: Mutex::new(HashMap::new()),
            confirmations: Mutex::new(HashMap::new()),
            command_rates: Mutex::new(CommandRateState::default()),
        }
    }

    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            self.run().await;
        });
    }

    pub async fn diagnostics(&self) -> TelegramBotDiagnostics {
        let mut diagnostics = self.diagnostics.lock().await.clone();
        diagnostics.audit_count = self.telegram_store.audit_count().await;
        diagnostics.pending_confirmations = self.confirmations.lock().await.len();
        diagnostics
    }

    pub async fn audits(&self, limit: usize) -> Vec<TelegramCommandAudit> {
        self.telegram_store.list_audits(limit).await
    }

    pub async fn webhook_matches(&self, path_secret: &str, header_secret: Option<&str>) -> bool {
        let settings = self.settings_store.get().await;
        settings.telegram_bot_mode == "webhook"
            && !settings.telegram_bot_webhook_path_secret.is_empty()
            && crate::utils::constant_time_eq(
                &settings.telegram_bot_webhook_path_secret,
                path_secret,
            )
            && header_secret.is_some_and(|provided| {
                !settings.telegram_bot_webhook_secret.is_empty()
                    && crate::utils::constant_time_eq(
                        &settings.telegram_bot_webhook_secret,
                        provided,
                    )
            })
    }

    pub async fn handle_update(&self, update: TelegramUpdate) {
        self.note_update().await;
        match self.telegram_store.claim_update(update.update_id).await {
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
        if let Some(callback) = update.callback_query {
            self.handle_callback(update.update_id, callback).await;
        } else if let Some(message) = update.message {
            self.handle_message(update.update_id, message).await;
        }
    }

    async fn handle_message(&self, update_id: i64, message: TelegramMessage) {
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
        let Some(text) = message.text.as_deref() else {
            return;
        };
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
            match self
                .prepare_confirmation(user.id, message.chat.id, command, argument)
                .await
            {
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
        } else {
            let page = argument
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            let response = self.command_response(command, page).await;
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
                    &format!("操作未执行：{}", outcome.1),
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

    async fn handle_callback(&self, update_id: i64, callback: TelegramCallbackQuery) {
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
                ("failed", format!("操作失败：{error}"))
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

    async fn run(self: Arc<Self>) {
        let mut offset: Option<i64> = None;
        let mut configured_fingerprint = String::new();
        let mut failures = 0_u32;
        let update_semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_UPDATES));
        loop {
            let settings = self.settings_store.get().await;
            self.set_mode(&settings.telegram_bot_mode).await;
            match settings.telegram_bot_mode.as_str() {
                "long_polling" if valid_common_config(&settings) => {
                    let fingerprint = format!("{}:long_polling", settings.telegram_bot_token);
                    if configured_fingerprint != fingerprint {
                        if let Err(error) = self.delete_webhook(&settings).await {
                            self.note_error(&error).await;
                            sleep_after_failure(&mut failures).await;
                            continue;
                        }
                        configured_fingerprint = fingerprint;
                        offset = None;
                    }
                    self.set_status("polling").await;
                    match self.get_updates(&settings, offset).await {
                        Ok(updates) => {
                            failures = 0;
                            for update in updates {
                                offset = Some(update.update_id.saturating_add(1));
                                // 每个 Update 独立处理，避免慢命令阻塞长轮询；
                                // 信号量限制并发上限（背压时在此等待，而非无界 spawn）。
                                let Ok(permit) = update_semaphore.clone().acquire_owned().await
                                else {
                                    break;
                                };
                                let service = self.clone();
                                tokio::spawn(async move {
                                    service.handle_update(update).await;
                                    drop(permit);
                                });
                            }
                        }
                        Err(error) => {
                            self.note_error(&error).await;
                            sleep_after_failure(&mut failures).await;
                        }
                    }
                }
                "webhook" if valid_webhook_config(&settings) => {
                    let fingerprint = format!(
                        "{}:{}:{}:{}",
                        settings.telegram_bot_token,
                        settings.telegram_bot_webhook_public_url,
                        settings.telegram_bot_webhook_path_secret,
                        settings.telegram_bot_webhook_secret
                    );
                    if configured_fingerprint != fingerprint {
                        match self.set_webhook(&settings).await {
                            Ok(()) => {
                                configured_fingerprint = fingerprint;
                                failures = 0;
                                self.set_status("webhook_active").await;
                            }
                            Err(error) => {
                                self.note_error(&error).await;
                                sleep_after_failure(&mut failures).await;
                                continue;
                            }
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                "disabled" | "" => {
                    configured_fingerprint.clear();
                    self.set_status("disabled").await;
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
                _ => {
                    configured_fingerprint.clear();
                    self.set_status("misconfigured").await;
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }

    async fn command_response(&self, command: &str, page: usize) -> String {
        match command {
            "start" => format!("my-media-sub Telegram 控制已连接。\n\n{}", help_text()),
            "help" => help_text().to_string(),
            "status" => self.status_text().await,
            "subscriptions" => self.subscriptions_text(page).await,
            "calendar" => self.calendar_text(page).await,
            "jobs" => self.jobs_text(page).await,
            "notifications" => self.notifications_text(page).await,
            "diagnostics" => self.diagnostics_text().await,
            _ => help_text().to_string(),
        }
    }

    async fn status_text(&self) -> String {
        let subscriptions = self.subscription_store.list().await;
        let jobs = self.job_store.list().await;
        let notifications = self.notification_store.list(false).await;
        let enabled = subscriptions.iter().filter(|item| item.enabled).count();
        let queued = jobs
            .iter()
            .filter(|job| job.status == JobStatus::Queued)
            .count();
        let running = jobs
            .iter()
            .filter(|job| job.status == JobStatus::Running)
            .count();
        let failed = jobs
            .iter()
            .filter(|job| job.status == JobStatus::Failed)
            .count();
        format!(
            "系统状态\n版本：{}\n订阅：{}（启用 {}）\n任务：排队 {} / 运行 {} / 失败 {}\n未读通知：{}",
            env!("CARGO_PKG_VERSION"),
            subscriptions.len(),
            enabled,
            queued,
            running,
            failed,
            notifications.len()
        )
    }

    async fn subscriptions_text(&self, page: usize) -> String {
        let items = self.subscription_store.list().await;
        let (start, end, page, pages) = page_bounds(items.len(), page);
        let mut lines = vec![format!(
            "订阅（第 {}/{} 页，共 {} 条）",
            page,
            pages,
            items.len()
        )];
        for item in &items[start..end] {
            let state = if !item.enabled {
                "停用"
            } else if item.completed {
                "完成"
            } else {
                "启用"
            };
            let progress = item
                .total_episode_number
                .map(|total| format!("{}/{}", item.current_episode_number, total))
                .unwrap_or_else(|| item.current_episode_number.to_string());
            lines.push(format!(
                "• {} {} [{}，进度 {}]",
                short_id(&item.id),
                item.title,
                state,
                progress
            ));
        }
        if items.is_empty() {
            lines.push("暂无订阅".to_string());
        }
        lines.join("\n")
    }

    async fn jobs_text(&self, page: usize) -> String {
        let mut items = self.job_store.list().await;
        items.sort_by_key(|job| std::cmp::Reverse(job.updated_at));
        let (start, end, page, pages) = page_bounds(items.len(), page);
        let mut lines = vec![format!(
            "任务（第 {}/{} 页，共 {} 条）",
            page,
            pages,
            items.len()
        )];
        for job in &items[start..end] {
            lines.push(format!(
                "• {} [{}，{}%] {}",
                short_id(&job.id),
                status_name(&job.status),
                job.progress,
                one_line(&job.title, 80)
            ));
        }
        if items.is_empty() {
            lines.push("暂无任务".to_string());
        }
        lines.join("\n")
    }

    async fn notifications_text(&self, page: usize) -> String {
        let items = self.notification_store.list(false).await;
        let (start, end, page, pages) = page_bounds(items.len(), page);
        let mut lines = vec![format!(
            "未读通知（第 {}/{} 页，共 {} 条）",
            page,
            pages,
            items.len()
        )];
        for item in &items[start..end] {
            lines.push(format!(
                "• {} [{}] {} — {}",
                short_id(&item.id),
                item.level,
                one_line(&item.title, 60),
                one_line(&item.message, 100)
            ));
        }
        if items.is_empty() {
            lines.push("暂无未读通知".to_string());
        }
        lines.join("\n")
    }

    async fn calendar_text(&self, page: usize) -> String {
        let today = Utc::now().with_timezone(&shanghai_offset()).date_naive();
        let (from, to) = natural_week(today);
        let (subscriptions, settings, jobs, notifications, events) = tokio::join!(
            self.subscription_store.list(),
            self.settings_store.get(),
            self.job_store.list(),
            self.notification_store.list(true),
            self.automation_event_store.list(5_000),
        );
        let calendar = build_media_calendar(
            subscriptions,
            &settings,
            &jobs,
            &notifications,
            &events,
            &MediaCalendarQuery {
                from,
                to,
                today,
                status: None,
                media_type: None,
                subscription_id: None,
            },
        );
        let (start, end, page, pages) = page_bounds(calendar.items.len(), page);
        let mut lines = vec![format!(
            "本周日历（第 {}/{} 页，共 {} 项）",
            page,
            pages,
            calendar.items.len()
        )];
        for item in &calendar.items[start..end] {
            let date = item.scheduled_date.as_deref().unwrap_or("日期未知");
            let episode = item
                .episode
                .map(|value| format!(" E{}", value))
                .unwrap_or_default();
            lines.push(format!(
                "• {} {}{} [{}]",
                date,
                one_line(&item.subscription_title, 70),
                episode,
                item.primary_status.as_str()
            ));
        }
        if calendar.items.is_empty() {
            lines.push("本周暂无排期".to_string());
        }
        lines.join("\n")
    }

    async fn diagnostics_text(&self) -> String {
        let state = self.diagnostics().await;
        let settings = self.settings_store.get().await;
        format!(
            "Telegram Bot 诊断\n接入模式：{}\n运行状态：{}\nToken：{}\n允许用户：{}\n允许聊天：{}\n仅私聊：{}\n最近 Update：{}\n最近成功：{}\n最近错误：{}\n未授权 Update：{}\n去重 Update/Callback：{}\n限流拒绝：{}\n命令审计：{}\n待确认：{}",
            state.mode,
            state.status,
            if settings.telegram_bot_token.is_empty() { "未配置" } else { "已配置（已脱敏）" },
            settings.telegram_bot_allowed_user_ids.len(),
            effective_allowed_chats(&settings).len(),
            if settings.telegram_bot_private_only { "是" } else { "否" },
            timestamp_text(state.last_update_at),
            timestamp_text(state.last_success_at),
            state.last_error.as_deref().unwrap_or("无"),
            state.unauthorized_updates,
            state.deduplicated_updates,
            state.rate_limited_updates,
            state.audit_count,
            state.pending_confirmations
        )
    }

    async fn get_updates(
        &self,
        settings: &Settings,
        offset: Option<i64>,
    ) -> Result<Vec<TelegramUpdate>, String> {
        let payload = json!({
            "offset": offset,
            "timeout": 25,
            "allowed_updates": ["message", "callback_query"]
        });
        self.telegram_request(settings, "getUpdates", &payload)
            .await
    }

    async fn send_message(
        &self,
        settings: &Settings,
        chat_id: i64,
        text: &str,
    ) -> Result<(), String> {
        self.send_message_with_markup(settings, chat_id, text, None)
            .await
    }

    async fn send_message_with_markup(
        &self,
        settings: &Settings,
        chat_id: i64,
        text: &str,
        reply_markup: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let mut payload = json!({
            "chat_id": chat_id,
            "text": text,
            "disable_web_page_preview": true
        });
        if let Some(reply_markup) = reply_markup {
            payload["reply_markup"] = reply_markup;
        }
        let _: serde_json::Value = self
            .telegram_request(settings, "sendMessage", &payload)
            .await?;
        Ok(())
    }

    async fn answer_callback(
        &self,
        settings: &Settings,
        callback_id: &str,
        text: &str,
        alert: bool,
    ) -> Result<(), String> {
        let _: serde_json::Value = self
            .telegram_request(
                settings,
                "answerCallbackQuery",
                &json!({"callback_query_id": callback_id, "text": one_line(text, 180), "show_alert": alert}),
            )
            .await?;
        Ok(())
    }

    async fn delete_webhook(&self, settings: &Settings) -> Result<(), String> {
        let _: serde_json::Value = self
            .telegram_request(
                settings,
                "deleteWebhook",
                &json!({"drop_pending_updates": false}),
            )
            .await?;
        Ok(())
    }

    async fn set_webhook(&self, settings: &Settings) -> Result<(), String> {
        let url = format!(
            "{}/api/telegram/webhook/{}",
            settings
                .telegram_bot_webhook_public_url
                .trim_end_matches('/'),
            settings.telegram_bot_webhook_path_secret
        );
        let payload = json!({
            "url": url,
            "secret_token": settings.telegram_bot_webhook_secret,
            "allowed_updates": ["message", "callback_query"],
            "drop_pending_updates": false
        });
        let _: serde_json::Value = self
            .telegram_request(settings, "setWebhook", &payload)
            .await?;
        Ok(())
    }

    async fn telegram_request<T: serde::de::DeserializeOwned>(
        &self,
        settings: &Settings,
        method: &str,
        payload: &serde_json::Value,
    ) -> Result<T, String> {
        let url = format!(
            "{}/bot{}/{}",
            self.api_base, settings.telegram_bot_token, method
        );
        let response = self
            .client
            .post(url)
            .json(payload)
            .send()
            .await
            .map_err(|error| sanitize_error_with_settings(&error.to_string(), settings))?;
        let status = response.status();
        let body = response
            .json::<TelegramApiResponse<T>>()
            .await
            .map_err(|error| sanitize_error_with_settings(&error.to_string(), settings))?;
        telegram_response_result(status, body, settings)
    }

    async fn note_update(&self) {
        self.diagnostics.lock().await.last_update_at = Some(crate::utils::unix_now());
    }

    async fn note_success(&self) {
        let mut diagnostics = self.diagnostics.lock().await;
        diagnostics.last_success_at = Some(crate::utils::unix_now());
        diagnostics.last_error = None;
    }

    async fn note_error(&self, error: &str) {
        let sanitized = sanitize_error(error);
        tracing::warn!(error = %sanitized, "Telegram Bot 接入失败");
        let mut diagnostics = self.diagnostics.lock().await;
        diagnostics.status = "error".to_string();
        diagnostics.last_error = Some(sanitized);
    }

    async fn set_mode(&self, mode: &str) {
        self.diagnostics.lock().await.mode = mode.to_string();
    }

    async fn set_status(&self, status: &str) {
        self.diagnostics.lock().await.status = status.to_string();
    }

    async fn audit_unauthorized(&self, user_id: Option<i64>, chat_id: i64, reason: &str) {
        let now = crate::utils::unix_now();
        self.diagnostics.lock().await.unauthorized_updates += 1;
        let key = format!("{}:{}", user_id.unwrap_or_default(), chat_id);
        let mut audits = self.security_audits.lock().await;
        audits.retain(|_, at| now.saturating_sub(*at) < SECURITY_AUDIT_INTERVAL_SECONDS * 2);
        if audits
            .get(&key)
            .is_some_and(|at| now.saturating_sub(*at) < SECURITY_AUDIT_INTERVAL_SECONDS)
            || (audits.len() >= 10_000 && !audits.contains_key(&key))
        {
            return;
        }
        audits.insert(key, now);
        tracing::warn!(user_id, chat_id, reason, "静默拒绝未授权 Telegram Update");
    }
}

include!("telegram_bot/commands.rs");

fn telegram_response_result<T>(
    status: reqwest::StatusCode,
    body: TelegramApiResponse<T>,
    settings: &Settings,
) -> Result<T, String> {
    if status.is_success() && body.ok {
        body.result
            .ok_or_else(|| "Telegram API 缺少 result".to_string())
    } else {
        Err(sanitize_error_with_settings(
            body.description
                .as_deref()
                .unwrap_or("Telegram API 返回失败"),
            settings,
        ))
    }
}

pub fn telegram_prompt_callback_data(
    settings: &Settings,
    action: &str,
    resource: &str,
    expires_at: i64,
) -> Option<String> {
    let code = match action {
        "check" => "c",
        "read" => "m",
        "view" => "v",
        "retry" => "r",
        _ => return None,
    };
    if resource.is_empty()
        || resource.len() > 36
        || !resource
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
        || settings.telegram_bot_allowed_user_ids.len() != 1
    {
        return None;
    }
    let chat_id = settings.telegram_chat_id.trim().parse::<i64>().ok()?;
    let user_id = settings.telegram_bot_allowed_user_ids[0];
    let expires = to_base36(expires_at.max(0) as u64);
    let material = format!("{code}|{resource}|{expires}|{user_id}|{chat_id}");
    let signature = telegram_callback_signature(settings, &material)?;
    let data = format!("prompt:{code}.{resource}.{expires}.{signature}");
    (data.len() <= 64).then_some(data)
}

fn verify_prompt_callback_data(
    settings: &Settings,
    token: &str,
    user_id: i64,
    chat_id: i64,
) -> Result<(String, String), String> {
    let mut parts = token.split('.');
    let code = parts.next().unwrap_or_default();
    let resource = parts.next().unwrap_or_default();
    let expires = parts.next().unwrap_or_default();
    let signature = parts.next().unwrap_or_default();
    if parts.next().is_some() || code.is_empty() || resource.is_empty() || signature.is_empty() {
        return Err("按钮数据无效".to_string());
    }
    let expires_at = from_base36(expires).ok_or_else(|| "按钮有效期无效".to_string())? as i64;
    if expires_at < crate::utils::unix_now() {
        return Err("按钮已过期".to_string());
    }
    let material = format!("{code}|{resource}|{expires}|{user_id}|{chat_id}");
    let expected = telegram_callback_signature(settings, &material)
        .ok_or_else(|| "按钮签名密钥不可用".to_string())?;
    if !crate::utils::constant_time_eq(&expected, signature) {
        return Err("按钮签名或会话不匹配".to_string());
    }
    let action = match code {
        "c" => "check",
        "m" => "read",
        "v" => "view",
        "r" => "retry",
        _ => return Err("按钮动作不受支持".to_string()),
    };
    Ok((action.to_string(), resource.to_string()))
}

fn telegram_callback_signature(settings: &Settings, material: &str) -> Option<String> {
    let secret = settings.telegram_bot_webhook_secret.as_bytes();
    if secret.len() < 24 {
        return None;
    }
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret);
    let signature = URL_SAFE_NO_PAD.encode(ring::hmac::sign(&key, material.as_bytes()).as_ref());
    Some(signature.chars().take(8).collect())
}

fn to_base36(mut value: u64) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let mut output = Vec::new();
    while value > 0 {
        let digit = (value % 36) as u8;
        output.push(if digit < 10 {
            char::from(b'0' + digit)
        } else {
            char::from(b'a' + digit - 10)
        });
        value /= 36;
    }
    output.iter().rev().collect()
}

fn from_base36(value: &str) -> Option<u64> {
    value.chars().try_fold(0_u64, |result, character| {
        character
            .to_digit(36)
            .map(|digit| result.saturating_mul(36).saturating_add(u64::from(digit)))
    })
}

fn valid_common_config(settings: &Settings) -> bool {
    !settings.telegram_bot_token.trim().is_empty()
        && !settings.telegram_bot_allowed_user_ids.is_empty()
        && !effective_allowed_chats(settings).is_empty()
}

fn valid_webhook_config(settings: &Settings) -> bool {
    valid_common_config(settings)
        && settings
            .telegram_bot_webhook_public_url
            .starts_with("https://")
        && settings.telegram_bot_webhook_path_secret.len() >= 24
        && settings.telegram_bot_webhook_secret.len() >= 24
}

fn effective_allowed_chats(settings: &Settings) -> Vec<i64> {
    let mut chats = settings.telegram_bot_allowed_chat_ids.clone();
    if let Ok(chat_id) = settings.telegram_chat_id.trim().parse::<i64>() {
        if !chats.contains(&chat_id) {
            chats.push(chat_id);
        }
    }
    chats
}

fn is_authorized(settings: &Settings, user_id: i64, chat: &TelegramChat) -> bool {
    settings.telegram_bot_allowed_user_ids.contains(&user_id)
        && effective_allowed_chats(settings).contains(&chat.id)
        && (!settings.telegram_bot_private_only || chat.kind == "private")
}

fn parse_command(text: &str) -> Option<(&'static str, Option<&str>)> {
    let mut parts = text.split_whitespace();
    let token = parts.next()?.strip_prefix('/')?;
    let command = token.split('@').next()?.to_ascii_lowercase();
    let supported = [
        "start",
        "help",
        "status",
        "subscriptions",
        "calendar",
        "jobs",
        "notifications",
        "diagnostics",
        "check",
        "retry",
        "cancel",
        "signin",
        "read",
    ];
    let command = supported
        .into_iter()
        .find(|item| *item == command)
        .unwrap_or("help");
    Some((command, parts.next()))
}

fn is_write_command(command: &str) -> bool {
    matches!(command, "check" | "retry" | "cancel" | "signin" | "read")
}

fn bot_action_scope(command: &str, resource: &str) -> Result<&'static str, String> {
    let path = match command {
        "check" if resource == "all" => "/api/subscriptions/check".to_string(),
        "check" => format!("/api/subscriptions/{resource}/check"),
        "retry" => format!("/api/jobs/{resource}/retry"),
        "cancel" => format!("/api/jobs/{resource}/cancel"),
        "signin" => "/api/quark/signin".to_string(),
        "read" if resource == "all" => "/api/notifications/read-all".to_string(),
        "read" => format!("/api/notifications/{resource}/read"),
        _ => return Err("操作没有对应的最小作用域".to_string()),
    };
    crate::api::required_token_scope(&axum::http::Method::POST, &path)
        .ok_or_else(|| "操作不在自动化最小作用域白名单中".to_string())
}

fn confirmation_prompt(confirmation: &PendingConfirmation) -> String {
    format!(
        "请确认操作\n动作：{}\n最小作用域：{}\n目标：{}\n有效期：{} 秒\n\n确认仅对当前 user/chat 有效，且只能使用一次。",
        confirmation.action, confirmation.scope, confirmation.resource, CONFIRMATION_TTL_SECONDS
    )
}

fn confirmation_markup(nonce: &str) -> serde_json::Value {
    json!({
        "inline_keyboard": [[
            {"text": "✅ 确认", "callback_data": format!("confirm:{nonce}")},
            {"text": "取消", "callback_data": format!("cancel:{nonce}")}
        ]]
    })
}

fn resolve_unique_id<'a>(
    ids: impl Iterator<Item = &'a str>,
    value: &str,
    label: &str,
) -> Result<String, String> {
    let matches = ids
        .filter(|id| *id == value || id.starts_with(value))
        .take(2)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [id] => Ok((*id).to_string()),
        [] => Err(format!("{label}不存在：{value}")),
        _ => Err(format!("{label} ID 前缀不唯一，请提供更长 ID")),
    }
}

fn page_bounds(total: usize, requested_page: usize) -> (usize, usize, usize, usize) {
    let pages = total.max(1).div_ceil(LIST_PAGE_SIZE);
    let page = requested_page.clamp(1, pages);
    let start = ((page - 1) * LIST_PAGE_SIZE).min(total);
    let end = (start + LIST_PAGE_SIZE).min(total);
    (start, end, page, pages)
}

fn split_message(value: &str) -> Vec<String> {
    if value.chars().count() <= TELEGRAM_MESSAGE_LIMIT {
        return vec![value.to_string()];
    }
    let mut parts = Vec::new();
    let mut current = String::new();
    for line in value.lines() {
        let additional = line.chars().count() + usize::from(!current.is_empty());
        if !current.is_empty() && current.chars().count() + additional > TELEGRAM_MESSAGE_LIMIT {
            parts.push(current);
            current = String::new();
        }
        if line.chars().count() > TELEGRAM_MESSAGE_LIMIT {
            for chunk in chunk_chars(line, TELEGRAM_MESSAGE_LIMIT) {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
                parts.push(chunk);
            }
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn chunk_chars(value: &str, limit: usize) -> Vec<String> {
    let chars = value.chars().collect::<Vec<_>>();
    chars
        .chunks(limit)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

fn help_text() -> &'static str {
    "只读命令：\n/start — 连接说明\n/help — 命令列表\n/status — 系统概况\n/subscriptions [页码] — 订阅列表\n/calendar [页码] — 本周排期\n/jobs [页码] — 最近任务\n/notifications [页码] — 未读通知\n/diagnostics — Bot 诊断

受控写命令（均需按钮确认）：
/check <订阅ID|all>
/retry <Job ID>
/cancel <Job ID>
/signin
/read <通知ID|all>"
}

fn status_name(status: &JobStatus) -> &'static str {
    match status {
        JobStatus::Queued => "排队",
        JobStatus::Running => "运行",
        JobStatus::Succeeded => "成功",
        JobStatus::Failed => "失败",
        JobStatus::Canceled => "取消",
    }
}

fn short_id(value: &str) -> &str {
    value.get(..value.len().min(8)).unwrap_or(value)
}

fn one_line(value: &str, limit: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() > limit {
        format!("{}…", normalized.chars().take(limit).collect::<String>())
    } else {
        normalized
    }
}

fn timestamp_text(value: Option<i64>) -> String {
    value
        .and_then(|timestamp| chrono::DateTime::from_timestamp(timestamp, 0))
        .map(|date| {
            date.with_timezone(&shanghai_offset())
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| "无".to_string())
}

fn sanitize_error_with_settings(value: &str, settings: &Settings) -> String {
    let mut sanitized = value.to_string();
    for secret in [
        settings.telegram_bot_token.as_str(),
        settings.telegram_bot_webhook_path_secret.as_str(),
        settings.telegram_bot_webhook_secret.as_str(),
    ] {
        if !secret.is_empty() {
            sanitized = sanitized.replace(secret, "***");
        }
    }
    sanitize_error(&sanitized)
}

fn sanitize_error(value: &str) -> String {
    let token_re = Regex::new(r"(?i)bot[0-9]+:[A-Za-z0-9_-]+")
        .expect("hard-coded Telegram token regex must compile");
    let credential_re =
        Regex::new(r"(?i)(cookie|token|password|secret|key|authorization)(\s*[:=]\s*)([^&\s,;]+)")
            .expect("hard-coded credential regex must compile");
    let bearer_re = Regex::new(r"(?i)bearer\s+[A-Za-z0-9._~+/-]+")
        .expect("hard-coded bearer regex must compile");
    let value = token_re.replace_all(value, "bot***");
    let value = credential_re.replace_all(&value, "$1$2***");
    let value = bearer_re.replace_all(&value, "Bearer ***");
    value.chars().take(300).collect()
}

async fn sleep_after_failure(failures: &mut u32) {
    *failures = failures.saturating_add(1);
    let seconds = 2_u64.saturating_pow((*failures).min(5)).min(60);
    tokio::time::sleep(Duration::from_secs(seconds)).await;
}

#[cfg(test)]
include!("telegram_bot/tests.rs");
