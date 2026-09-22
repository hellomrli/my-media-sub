use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use chrono::Utc;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{Mutex, Semaphore};

mod auth;
mod callback_sign;
mod dispatch;
mod format;

// 三个子模块的函数都是 `pub(super)`，只在本模块内使用，因此用私有 `use` 引入。
// `include!` 进来的 commands.rs / menus.rs 文本上位于本模块内，同样能看到这些名字。
use auth::*;
use callback_sign::*;
use format::*;

// `push.rs` 通过 `crate::services::telegram_bot::telegram_prompt_callback_data`
// 构造通知按钮，因此这一项需要对外可见。
pub(crate) use callback_sign::telegram_prompt_callback_data;

use crate::jobs::{JobQueue, JobStatus, JobStore};
use crate::models::{Settings, Subscription};
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
    /// 确认时目标资源的快照指纹。对依赖搜索会话序号间接引用的动作
    /// （subscribe/transfer），执行时必须校验会话中的结果与确认时一致，
    /// 防止确认期间会话被新搜索覆盖后转存/订阅到错误资源。
    resource_fingerprint: String,
    /// 确认弹窗展示用的人类可读目标描述（标题 + 链接摘要）。
    resource_label: String,
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
    sessions: Mutex<SessionStore>,
    /// webhook 模式的并发上限：与 long polling 的 Semaphore(8) 同一背压，
    /// 防止断线恢复后的 update 突发转化为无上限的并发任务。
    update_semaphore: Arc<Semaphore>,
}

/// 分页列表的渲染结果，供翻页按钮与命令响应共用。
struct ListPage {
    text: String,
    page: usize,
    pages: usize,
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
            sessions: Mutex::new(SessionStore::default()),
            update_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_UPDATES)),
        }
    }

    pub fn start(self: Arc<Self>) {
        // spawn_supervised：长轮询循环 panic 会让**整个 Telegram 控制静默失效**
        // （命令、按钮、审计全部停止），此前没有任何机制会发现。
        let service = self.clone();
        crate::utils::spawn_supervised("Telegram Bot 长轮询", move || {
            let service = service.clone();
            async move { service.run().await }
        });
    }

    /// webhook 入口的受并发上限保护的 update 处理。
    pub async fn handle_update_bounded(&self, update: TelegramUpdate) {
        // 服务关闭时信号量关闭：acquire 出错则直接丢弃，与停机语义一致。
        if self.update_semaphore.acquire().await.is_ok() {
            self.handle_update(update).await;
        }
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
                    // 切到 disabled 时撤销 Telegram 侧 webhook：否则 Telegram
                    // 会持续 POST 得到 404 并指数退避重试，pending updates
                    // 堆积到重新启用后集中涌入。
                    if configured_fingerprint != "disabled" {
                        if let Err(error) = self.delete_webhook(&settings).await {
                            self.note_error(&error).await;
                        }
                        configured_fingerprint = "disabled".to_string();
                    }
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

    async fn command_response(&self, command: &str, argument: Option<&str>, page: usize) -> String {
        match command {
            "start" => format!("my-media-sub Telegram 控制已连接。\n\n{}", help_text()),
            "help" => help_text().to_string(),
            "status" => self.status_text().await,
            "subscriptions" => self.subscriptions_page(page).await.text,
            "subscription" => self.subscription_text(argument).await,
            "calendar" => self.calendar_page(page).await.text,
            "jobs" => self.jobs_page(page).await.text,
            "job" => self.job_text(argument).await,
            "notifications" => self.notifications_page(page).await.text,
            "diagnostics" => self.diagnostics_text().await,
            _ => help_text().to_string(),
        }
    }

    /// 分页列表的统一返回：正文 + 当前页码 + 总页数，供翻页按钮使用。
    async fn list_page(&self, command: &str, page: usize) -> ListPage {
        match command {
            "subscriptions" => self.subscriptions_page(page).await,
            "jobs" => self.jobs_page(page).await,
            "notifications" => self.notifications_page(page).await,
            "calendar" => self.calendar_page(page).await,
            _ => ListPage {
                text: help_text().to_string(),
                page: 1,
                pages: 1,
            },
        }
    }

    async fn subscription_text(&self, argument: Option<&str>) -> String {
        let Some(value) = argument else {
            return "用法：/subscription [订阅ID]".to_string();
        };
        let id = match self.resolve_subscription(value).await {
            Ok(id) => id,
            Err(error) => return error,
        };
        let Some(item) = self.subscription_store.get(&id).await else {
            return format!("订阅不存在：{}", tg_escape(value));
        };
        let progress = item
            .total_episode_number
            .map(|total| format!("{}/{}", item.current_episode_number, total))
            .unwrap_or_else(|| item.current_episode_number.to_string());
        let target_dir = if item.rules.target_dir.trim().is_empty() {
            "自动".to_string()
        } else {
            tg_escape(&item.rules.target_dir)
        };
        format!(
            "📺 <b>订阅详情</b>\n\nID：<code>{}</code>\n标题：{}\n类型：{}\n状态：{}\n进度：{}\n目标目录：{}\n上次检查：{}\n最近结果：{}\n\n操作：/check {}",
            tg_escape(&item.id),
            tg_escape(&item.title),
            if item.media_type == "movie" { "电影" } else { "剧集" },
            subscription_state_label(&item),
            tg_escape(&progress),
            target_dir,
            timestamp_text(Some(item.last_checked_at)),
            tg_escape(&one_line(&item.last_check_summary, 180)),
            short_id(&item.id)
        )
    }

    async fn job_text(&self, argument: Option<&str>) -> String {
        let Some(value) = argument else {
            return "用法：/job [Job ID]".to_string();
        };
        let id = match self.resolve_job(value).await {
            Ok(id) => id,
            Err(error) => return error,
        };
        let Some(job) = self.job_store.get(&id).await else {
            return format!("任务不存在：{}", tg_escape(value));
        };
        let error = job
            .error
            .as_deref()
            .map(|value| one_line(value, 240))
            .unwrap_or_else(|| "无".to_string());
        let error_class = job
            .error_class
            .as_ref()
            .map(job_error_class_label)
            .unwrap_or("—");
        format!(
            "⚙️ <b>任务详情</b>\n\nID：<code>{}</code>\n标题：{}\n类型：{}\n状态：{}\n进度：{}%\n尝试：{}\n错误分类：{}\n消息：{}\n错误：{}\n创建：{}\n开始：{}\n结束：{}\ncorrelation：<code>{}</code>\n\n可用操作：/retry {} 或 /cancel {}",
            tg_escape(&job.id),
            tg_escape(&one_line(&job.title, 100)),
            tg_escape(job.kind.as_str()),
            status_name(&job.status),
            job.progress,
            job.attempt,
            tg_escape(error_class),
            tg_escape(&one_line(&job.message, 180)),
            tg_escape(&error),
            timestamp_text(job.created_at.into()),
            timestamp_text(job.started_at),
            timestamp_text(job.finished_at),
            tg_escape(job.correlation_id.as_deref().unwrap_or("无")),
            short_id(&job.id),
            short_id(&job.id)
        )
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
            "📊 <b>系统状态</b>\n\n🖥 版本：{}\n📺 订阅：{}（启用 {}）\n⚙️ 任务：排队 {} / 运行 {} / 失败 {}\n🔔 未读通知：{}",
            env!("CARGO_PKG_VERSION"),
            subscriptions.len(),
            enabled,
            queued,
            running,
            failed,
            notifications.len()
        )
    }

    async fn subscriptions_page(&self, page: usize) -> ListPage {
        let items = self.subscription_store.list().await;
        let (start, end, page, pages) = page_bounds(items.len(), page);
        let mut lines = vec![format!(
            "📋 <b>订阅</b>（第 {page}/{pages} 页，共 {} 条）",
            items.len()
        )];
        for item in &items[start..end] {
            let progress = item
                .total_episode_number
                .map(|total| format!("{}/{}", item.current_episode_number, total))
                .unwrap_or_else(|| item.current_episode_number.to_string());
            lines.push(format!(
                "• <code>{}</code> {} — {} · 进度 {}",
                short_id(&item.id),
                tg_escape(&one_line(&item.title, 60)),
                subscription_state_label(item),
                tg_escape(&progress)
            ));
        }
        if items.is_empty() {
            lines.push("暂无订阅".to_string());
        }
        ListPage {
            text: lines.join("\n"),
            page,
            pages,
        }
    }

    async fn jobs_page(&self, page: usize) -> ListPage {
        let mut items = self.job_store.list().await;
        items.sort_by_key(|job| std::cmp::Reverse(job.updated_at));
        let (start, end, page, pages) = page_bounds(items.len(), page);
        let mut lines = vec![format!(
            "⚙️ <b>任务</b>（第 {page}/{pages} 页，共 {} 条）",
            items.len()
        )];
        for job in &items[start..end] {
            lines.push(format!(
                "• <code>{}</code> {} [{}%，{}] {}",
                short_id(&job.id),
                status_name(&job.status),
                job.progress,
                tg_escape(job.kind.as_str()),
                tg_escape(&one_line(&job.title, 60))
            ));
        }
        if items.is_empty() {
            lines.push("暂无任务".to_string());
        }
        ListPage {
            text: lines.join("\n"),
            page,
            pages,
        }
    }

    async fn notifications_page(&self, page: usize) -> ListPage {
        let items = self.notification_store.list(false).await;
        let (start, end, page, pages) = page_bounds(items.len(), page);
        let mut lines = vec![format!(
            "🔔 <b>未读通知</b>（第 {page}/{pages} 页，共 {} 条）",
            items.len()
        )];
        for item in &items[start..end] {
            lines.push(format!(
                "• <code>{}</code> {} {} — {}",
                short_id(&item.id),
                notification_level_label(&item.level),
                tg_escape(&one_line(&item.title, 50)),
                tg_escape(&one_line(&item.message, 90))
            ));
        }
        if items.is_empty() {
            lines.push("暂无未读通知".to_string());
        }
        ListPage {
            text: lines.join("\n"),
            page,
            pages,
        }
    }

    async fn calendar_page(&self, page: usize) -> ListPage {
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
            "📅 <b>本周日历</b>（第 {page}/{pages} 页，共 {} 项）",
            calendar.items.len()
        )];
        for item in &calendar.items[start..end] {
            let date = item.scheduled_date.as_deref().unwrap_or("日期未知");
            let season = if item.season > 0 {
                format!(" S{}", item.season)
            } else {
                String::new()
            };
            let episode = item
                .episode
                .map(|value| format!(" E{}", value))
                .unwrap_or_default();
            lines.push(format!(
                "• {} {} {}{} — {}",
                date,
                tg_escape(&one_line(&item.subscription_title, 60)),
                season,
                episode,
                calendar_status_label(item.primary_status.as_str())
            ));
        }
        if calendar.items.is_empty() {
            lines.push("本周暂无排期".to_string());
        }
        ListPage {
            text: lines.join("\n"),
            page,
            pages,
        }
    }

    async fn diagnostics_text(&self) -> String {
        let state = self.diagnostics().await;
        let settings = self.settings_store.get().await;
        format!(
            "🩺 <b>Telegram Bot 诊断</b>\n\n接入模式：{}\n运行状态：{}\nToken：{}\n允许用户：{}\n允许聊天：{}\n仅私聊：{}\n最近 Update：{}\n最近成功：{}\n最近错误：{}\n未授权 Update：{}\n去重 Update/Callback：{}\n限流拒绝：{}\n命令审计：{}\n待确认：{}",
            tg_escape(&state.mode),
            tg_escape(&state.status),
            if settings.telegram_bot_token.is_empty() { "未配置" } else { "已配置（已脱敏）" },
            settings.telegram_bot_allowed_user_ids.len(),
            effective_allowed_chats(&settings).len(),
            if settings.telegram_bot_private_only { "是" } else { "否" },
            timestamp_text(state.last_update_at),
            timestamp_text(state.last_success_at),
            tg_escape(state.last_error.as_deref().unwrap_or("无")),
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
            "parse_mode": "HTML",
            "disable_web_page_preview": true
        });
        if let Some(reply_markup) = reply_markup {
            payload["reply_markup"] = reply_markup;
        }
        match self
            .telegram_request::<serde_json::Value>(settings, "sendMessage", &payload)
            .await
        {
            Ok(_) => Ok(()),
            Err(error) if telegram_parse_error(&error) => {
                if let Some(object) = payload.as_object_mut() {
                    object.remove("parse_mode");
                }
                let _: serde_json::Value = self
                    .telegram_request(settings, "sendMessage", &payload)
                    .await?;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// 编辑已发送消息（分页按钮用），保持与 sendMessage 相同的 HTML 排版。
    async fn edit_message_with_markup(
        &self,
        settings: &Settings,
        chat_id: i64,
        message_id: i64,
        text: &str,
        reply_markup: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let mut payload = json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "text": text,
            "parse_mode": "HTML",
            "disable_web_page_preview": true
        });
        match reply_markup {
            Some(reply_markup) => payload["reply_markup"] = reply_markup,
            // 编辑消息时省略 reply_markup 会保留旧键盘；
            // 列表缩到单页后必须显式移除，否则翻页按钮会残留。
            None => payload["reply_markup"] = json!({"inline_keyboard": []}),
        }
        match self
            .telegram_request::<serde_json::Value>(settings, "editMessageText", &payload)
            .await
        {
            Ok(_) => Ok(()),
            Err(error) if telegram_parse_error(&error) => {
                if let Some(object) = payload.as_object_mut() {
                    object.remove("parse_mode");
                }
                let _: serde_json::Value = self
                    .telegram_request(settings, "editMessageText", &payload)
                    .await?;
                Ok(())
            }
            Err(error) => Err(error),
        }
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
include!("telegram_bot/menus.rs");

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

fn parse_command(text: &str) -> Option<(&'static str, Option<&str>)> {
    let text = text.trim();
    let (token_raw, rest) = match text.split_once(char::is_whitespace) {
        Some((head, tail)) => (head, tail.trim()),
        None => (text, ""),
    };
    let token = token_raw.strip_prefix('/')?;
    let command = token.split('@').next()?.to_ascii_lowercase();
    let supported = [
        "start",
        "menu",
        "help",
        "search",
        "subscribe",
        "switch",
        "switch_apply",
        "status",
        "subscriptions",
        "subscription",
        "calendar",
        "jobs",
        "job",
        "notifications",
        "diagnostics",
        "check",
        "retry",
        "cancel",
        "signin",
        "read",
        "transfer",
    ];
    let command = supported
        .into_iter()
        .find(|item| *item == command)
        .unwrap_or("help");
    let argument = (!rest.is_empty()).then_some(rest);
    Some((command, argument))
}

fn is_write_command(command: &str) -> bool {
    matches!(
        command,
        "check"
            | "retry"
            | "cancel"
            | "signin"
            | "read"
            | "subscribe"
            | "switch_apply"
            | "transfer"
            | "download"
    )
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
        "transfer" => return Ok("subscriptions:write"),
        "download" => return Ok("jobs:write"),
        _ => return Err("操作没有对应的最小作用域".to_string()),
    };
    crate::api::required_token_scope(&axum::http::Method::POST, &path)
        .ok_or_else(|| "操作不在自动化最小作用域白名单中".to_string())
}

fn confirmation_prompt(confirmation: &PendingConfirmation) -> String {
    let action_label = match confirmation.action.as_str() {
        "subscribe" => "创建订阅",
        "switch_apply" => "应用换源",
        "transfer" => "转存资源",
        "download" => "提交下载",
        "check" => "检查订阅",
        "retry" => "重试任务",
        "cancel" => "取消任务",
        "signin" => "夸克签到",
        "read" => "标记已读",
        other => other,
    };
    let target = if confirmation.resource_label.is_empty() {
        confirmation.resource.clone()
    } else {
        format!(
            "{}\n目标详情：{}",
            confirmation.resource, confirmation.resource_label
        )
    };
    format!(
        "🔐 <b>请确认操作</b>\n\n动作：<b>{}</b>\n目标：<code>{}</code>\n最小权限：<code>{}</code>\n有效期：{} 秒\n\n确认仅对当前会话有效，且只能使用一次。",
        action_label,
        tg_escape(&target),
        tg_escape(&confirmation.scope),
        CONFIRMATION_TTL_SECONDS
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
    static TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)bot[0-9]+:[A-Za-z0-9_-]+")
            .expect("hard-coded Telegram token regex must compile")
    });
    static CREDENTIAL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(cookie|token|password|secret|key|authorization)(\s*[:=]\s*)([^&\s,;]+)")
            .expect("hard-coded credential regex must compile")
    });
    static BEARER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)bearer\s+[A-Za-z0-9._~+/-]+")
            .expect("hard-coded bearer regex must compile")
    });
    let value = TOKEN_RE.replace_all(value, "bot***");
    let value = CREDENTIAL_RE.replace_all(&value, "$1$2***");
    let value = BEARER_RE.replace_all(&value, "Bearer ***");
    value.chars().take(300).collect()
}

async fn sleep_after_failure(failures: &mut u32) {
    *failures = failures.saturating_add(1);
    let seconds = 2_u64.saturating_pow((*failures).min(5)).min(60);
    tokio::time::sleep(Duration::from_secs(seconds)).await;
}

#[cfg(test)]
include!("telegram_bot/tests.rs");
