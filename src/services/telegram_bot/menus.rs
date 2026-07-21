use crate::clients::pansou::{PanSouClient, SearchResult};
use crate::models::subscription::{parse_season_spec, Subscription};
use crate::services::subscription_source_switch::SubscriptionSourceSwitchService;
use crate::services::title_normalize::clean_media_title;
use crate::utils::unix_now;

const SESSION_TTL_SECONDS: i64 = 15 * 60;
const MAX_SEARCH_RESULTS: usize = 5;
const MAX_SWITCH_CANDIDATES: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SearchHit {
    title: String,
    url: String,
    password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SwitchHit {
    candidate_id: String,
    note: String,
    url: String,
    score: i32,
}

#[derive(Debug, Clone)]
enum SessionKind {
    Search { hits: Vec<SearchHit> },
    Switch {
        subscription_id: String,
        hits: Vec<SwitchHit>,
    },
}

#[derive(Debug, Clone)]
struct UserSession {
    user_id: i64,
    chat_id: i64,
    expires_at: i64,
    kind: SessionKind,
}

#[derive(Default)]
struct SessionStore {
    sessions: HashMap<String, UserSession>,
}

impl SessionStore {
    fn key(user_id: i64, chat_id: i64) -> String {
        format!("{user_id}:{chat_id}")
    }

    fn put(&mut self, session: UserSession) {
        let now = unix_now();
        self.sessions.retain(|_, item| item.expires_at >= now);
        if self.sessions.len() >= 500 {
            if let Some(oldest) = self
                .sessions
                .iter()
                .min_by_key(|(_, item)| item.expires_at)
                .map(|(key, _)| key.clone())
            {
                self.sessions.remove(&oldest);
            }
        }
        self.sessions
            .insert(Self::key(session.user_id, session.chat_id), session);
    }

    fn get(&mut self, user_id: i64, chat_id: i64) -> Option<UserSession> {
        let now = unix_now();
        self.sessions.retain(|_, item| item.expires_at >= now);
        self.sessions.get(&Self::key(user_id, chat_id)).cloned()
    }

    fn put_loaded(&mut self, session: UserSession) {
        self.sessions
            .insert(Self::key(session.user_id, session.chat_id), session);
    }
}

fn main_menu_markup() -> serde_json::Value {
    json!({
        "keyboard": [
            [{"text": "🔍 搜索"}, {"text": "📋 订阅"}],
            [{"text": "🔄 检查全部"}, {"text": "📅 日历"}],
            [{"text": "⚙️ 状态"}, {"text": "❓ 帮助"}]
        ],
        "resize_keyboard": true,
        "is_persistent": true
    })
}

fn bot_commands_payload() -> serde_json::Value {
    json!({
        "commands": [
            {"command": "start", "description": "连接说明与主菜单"},
            {"command": "menu", "description": "显示主菜单"},
            {"command": "help", "description": "命令列表"},
            {"command": "search", "description": "搜索夸克资源"},
            {"command": "subscribe", "description": "订阅最近搜索结果"},
            {"command": "switch", "description": "搜索换源候选"},
            {"command": "switch_apply", "description": "应用换源候选"},
            {"command": "status", "description": "系统概况"},
            {"command": "subscriptions", "description": "订阅列表"},
            {"command": "subscription", "description": "订阅详情"},
            {"command": "calendar", "description": "本周排期"},
            {"command": "jobs", "description": "最近任务"},
            {"command": "job", "description": "任务详情"},
            {"command": "notifications", "description": "未读通知"},
            {"command": "diagnostics", "description": "Bot 诊断"},
            {"command": "check", "description": "检查订阅（需确认）"},
            {"command": "retry", "description": "重试任务（需确认）"},
            {"command": "cancel", "description": "取消任务（需确认）"},
            {"command": "signin", "description": "夸克签到（需确认）"},
            {"command": "read", "description": "标记通知已读（需确认）"}
        ]
    })
}

fn map_menu_text(text: &str) -> Option<&'static str> {
    match text.trim() {
        "🔍 搜索" | "搜索" => Some("__search_help__"),
        "📋 订阅" | "订阅" | "订阅列表" => Some("/subscriptions"),
        "🔄 检查全部" | "检查全部" => Some("/check all"),
        "📅 日历" | "日历" => Some("/calendar"),
        "⚙️ 状态" | "状态" => Some("/status"),
        "❓ 帮助" | "帮助" => Some("/help"),
        "菜单" | "主菜单" => Some("/menu"),
        _ => None,
    }
}

fn format_search_hits(keyword: &str, hits: &[SearchHit]) -> String {
    if hits.is_empty() {
        return format!("未找到与「{keyword}」相关的夸克资源。");
    }
    let mut lines = vec![
        format!("搜索「{keyword}」共 {} 条（15 分钟内有效）：", hits.len()),
        String::new(),
    ];
    for (index, hit) in hits.iter().enumerate() {
        lines.push(format!(
            "{}. {}\n{}",
            index + 1,
            one_line(&hit.title, 80),
            one_line(&hit.url, 120)
        ));
    }
    lines.push(String::new());
    lines.push(
        "订阅：/subscribe <序号> [季号]\n例如：/subscribe 1  或  /subscribe 1 1-4".to_string(),
    );
    lines.join("\n")
}

fn format_switch_hits(sub: &Subscription, hits: &[SwitchHit]) -> String {
    if hits.is_empty() {
        return format!("订阅 {} 未找到换源候选。", sub.title);
    }
    let mut lines = vec![
        format!(
            "换源候选 · {}\nID：{}\n共 {} 条（15 分钟内有效）：",
            one_line(&sub.title, 60),
            short_id(&sub.id),
            hits.len()
        ),
        String::new(),
    ];
    for (index, hit) in hits.iter().enumerate() {
        lines.push(format!(
            "{}. {} 分 · {}\n{}",
            index + 1,
            hit.score,
            one_line(&hit.note, 70),
            one_line(&hit.url, 120)
        ));
    }
    lines.push(String::new());
    lines.push("应用：/switch_apply <序号>\n例如：/switch_apply 1（需确认）".to_string());
    lines.join("\n")
}

fn search_help_text() -> &'static str {
    "搜索资源：\n/search <关键词>\n例如：/search 庆余年\n\n找到结果后用：\n/subscribe <序号> [季号]\n例如：/subscribe 1 1-4"
}

fn switch_help_text() -> &'static str {
    "换源：\n/switch <订阅ID>\n例如：/switch a1b2c3d4\n\n找到候选后用：\n/switch_apply <序号>\n例如：/switch_apply 1"
}

impl TelegramBotService {
    async fn ensure_bot_commands(&self, settings: &Settings) {
        let _: Result<serde_json::Value, String> = self
            .telegram_request(settings, "setMyCommands", &bot_commands_payload())
            .await;
    }

    async fn search_resources_text(&self, keyword: &str, user_id: i64, chat_id: i64) -> String {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return search_help_text().to_string();
        }
        let settings = self.settings_store.get().await;
        let pansou = settings.pansou_api_url.trim();
        if pansou.is_empty() {
            return "未配置 PanSou API，无法搜索。".to_string();
        }
        let client = PanSouClient::new(Some(pansou.to_string()));
        let results = match client
            .search(keyword, &["quark".to_string()], MAX_SEARCH_RESULTS)
            .await
        {
            Ok(items) => items,
            Err(error) => return format!("搜索失败：{}", sanitize_error(&error.to_string())),
        };
        let hits = results
            .into_iter()
            .take(MAX_SEARCH_RESULTS)
            .map(|item: SearchResult| SearchHit {
                title: if item.display_title.trim().is_empty() {
                    clean_media_title(&item.note)
                } else {
                    item.display_title
                },
                url: item.url,
                password: item.password,
            })
            .collect::<Vec<_>>();
        let expires_at = unix_now() + SESSION_TTL_SECONDS;
        let session = UserSession {
            user_id,
            chat_id,
            expires_at,
            kind: SessionKind::Search {
                hits: hits.clone(),
            },
        };
        self.sessions.lock().await.put(session.clone());
        let _ = self
            .telegram_store
            .put_user_session(crate::store::telegram_bot::TelegramUserSessionRecord {
                user_id,
                chat_id,
                expires_at,
                kind: "search".to_string(),
                payload: serde_json::to_string(&hits).unwrap_or_default(),
            })
            .await;
        let mut text = format_search_hits(keyword, &hits);
        // 内联按钮：点选序号订阅
        let mut rows = Vec::new();
        let mut row = Vec::new();
        for index in 1..=hits.len() {
            row.push(json!({
                "text": format!("订阅 {index}"),
                "callback_data": format!("msub:{index}")
            }));
            if row.len() == 3 {
                rows.push(std::mem::take(&mut row));
            }
        }
        if !row.is_empty() {
            rows.push(row);
        }
        if !rows.is_empty() {
            let markup = json!({ "inline_keyboard": rows });
            let _ = self
                .send_message_with_markup(
                    &self.settings_store.get().await,
                    chat_id,
                    "点按钮选择要订阅的结果：",
                    Some(markup),
                )
                .await;
            text.push_str("\n\n也可点击上方按钮，或发送 /subscribe <序号> [季号]。");
        }
        text
    }

    async fn subscribe_prepare(
        &self,
        argument: Option<&str>,
        user_id: i64,
        chat_id: i64,
    ) -> Result<PendingConfirmation, String> {
        let argument = argument.ok_or_else(|| {
            "用法：/subscribe <序号> [季号]\n先用 /search <关键词>，再 /subscribe 1 1-4".to_string()
        })?;
        let mut parts = argument.split_whitespace();
        let index = parts
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .ok_or_else(|| "序号必须是从 1 开始的数字".to_string())?;
        let season_spec = parts.next().unwrap_or("1");
        let _ = parse_season_spec(season_spec); // validate
        let session = self
            .load_session(user_id, chat_id)
            .await
            .ok_or_else(|| "没有可用的搜索结果，请先 /search <关键词>".to_string())?;
        let SessionKind::Search { hits } = &session.kind else {
            return Err("当前会话不是搜索结果，请先 /search <关键词>".to_string());
        };
        let hit = hits
            .get(index - 1)
            .ok_or_else(|| format!("序号超出范围（1-{}）", hits.len()))?;
        let resource = format!("{index}|{season_spec}|{}", short_id(&hit.url));
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let confirmation = PendingConfirmation {
            nonce: nonce.clone(),
            user_id,
            chat_id,
            action: "subscribe".to_string(),
            scope: "subscriptions:write".to_string(),
            resource,
            expires_at: unix_now() + CONFIRMATION_TTL_SECONDS,
            idempotency_key: format!(
                "telegram:subscriptions:write:{user_id}:{chat_id}:subscribe:{index}:{nonce}"
            ),
        };
        let mut confirmations = self.confirmations.lock().await;
        let now = unix_now();
        confirmations.retain(|_, item| item.expires_at >= now);
        if confirmations.len() >= 1_000 {
            return Err("待确认操作过多，请稍后再试".to_string());
        }
        confirmations.insert(nonce, confirmation.clone());
        Ok(confirmation)
    }

    async fn execute_subscribe(
        &self,
        user_id: i64,
        chat_id: i64,
        resource: &str,
    ) -> Result<String, String> {
        let mut parts = resource.split('|');
        let index = parts
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .ok_or_else(|| "订阅参数无效".to_string())?;
        let season_spec = parts.next().unwrap_or("1");
        let (season, season_end) = parse_season_spec(season_spec);
        let session = self
            .load_session(user_id, chat_id)
            .await
            .ok_or_else(|| "搜索会话已过期，请重新 /search".to_string())?;
        let SessionKind::Search { hits } = session.kind else {
            return Err("搜索会话已失效，请重新 /search".to_string());
        };
        let hit = hits
            .get(index - 1)
            .cloned()
            .ok_or_else(|| "搜索结果序号无效".to_string())?;

        let settings = self.settings_store.get().await;
        let title = if hit.title.trim().is_empty() {
            clean_media_title(&hit.url)
        } else {
            hit.title.clone()
        };
        let id = format!("{:x}", md5::compute(format!("{}:{}", hit.url, title)));
        let id = id[..12].to_string();
        let now = unix_now();
        let mut rules = crate::models::rules::TransferRules::default();
        if season_end.is_some() {
            let show = if title.is_empty() {
                "未命名".to_string()
            } else {
                title.clone()
            };
            let base = settings.quark_save_series_dir.trim();
            rules.target_dir = if base.is_empty() {
                format!("/{show}")
            } else {
                format!("{}/{}", base.trim_end_matches('/'), show)
            };
        }

        let subscription = Subscription {
            id: id.clone(),
            title: title.clone(),
            source_title: hit.title.clone(),
            media_type: "series".to_string(),
            season,
            season_end,
            start_episode_number: None,
            current_episode_number: 0,
            total_episode_number: None,
            source_group: String::new(),
            tags: vec![],
            metadata: None,
            manual_schedule: None,
            cloud_type: "quark".to_string(),
            url: hit.url.clone(),
            password: hit.password.clone(),
            known_files: vec![],
            known_file_keys: vec![],
            known_episodes: vec![],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            last_probe: None,
            last_plan_summary: String::new(),
            notify_only: false,
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            sync_downloads: vec![],
            strm_enabled: false,
            enabled: true,
            completed: false,
            rules,
            rule_preset_id: String::new(),
            created_at: now,
            updated_at: now,
            last_checked_at: now,
            last_new_files: vec![],
            last_new_episodes: vec![],
            last_check_summary: String::new(),
            check_history: vec![],
            status: "active".to_string(),
            invalid_since: None,
            last_error: String::new(),
            rule_summary: String::new(),
            source_candidates: vec![],
            last_source_search_time: None,
            previous_share_links: vec![],
            source_failure_count: 0,
            last_source_switch_at: None,
            source_switch_history: vec![],
        };

        let created = self
            .subscription_store
            .create(subscription)
            .await
            .map_err(|error| sanitize_error(&error.to_string()))?;

        // 后台刮削元数据（失败不影响订阅创建）
        let _ = self
            .job_queue
            .submit_metadata_scrape(crate::jobs::MetadataScrapePayload {
                subscription_id: Some(created.id.clone()),
                overwrite: false,
            })
            .await;

        Ok(format!(
            "已创建订阅\n标题：{}\nID：{}\n季：{}\n链接：{}\n已提交元数据刮削任务\n\n立即检查：/check {}\n换源：/switch {}",
            created.title,
            created.id,
            created.season_label(),
            one_line(&created.url, 120),
            short_id(&created.id),
            short_id(&created.id)
        ))
    }

    async fn load_session(&self, user_id: i64, chat_id: i64) -> Option<UserSession> {
        if let Some(session) = self.sessions.lock().await.get(user_id, chat_id) {
            return Some(session);
        }
        let record = self.telegram_store.get_user_session(user_id, chat_id).await?;
        let kind = match record.kind.as_str() {
            "search" => {
                let hits: Vec<SearchHit> = serde_json::from_str(&record.payload).ok()?;
                SessionKind::Search { hits }
            }
            "switch" => {
                #[derive(Deserialize)]
                struct SwitchPayload {
                    subscription_id: String,
                    hits: Vec<SwitchHit>,
                }
                let payload: SwitchPayload = serde_json::from_str(&record.payload).ok()?;
                SessionKind::Switch {
                    subscription_id: payload.subscription_id,
                    hits: payload.hits,
                }
            }
            _ => return None,
        };
        let session = UserSession {
            user_id,
            chat_id,
            expires_at: record.expires_at,
            kind,
        };
        self.sessions.lock().await.put_loaded(session.clone());
        Some(session)
    }

    async fn switch_search_text(&self, argument: Option<&str>, user_id: i64, chat_id: i64) -> String {
        let Some(value) = argument else {
            return switch_help_text().to_string();
        };
        let id = match self.resolve_subscription(value).await {
            Ok(id) => id,
            Err(error) => return error,
        };
        let Some(sub) = self.subscription_store.get(&id).await else {
            return format!("订阅不存在：{value}");
        };
        let settings = self.settings_store.get().await;
        let pansou = settings.pansou_api_url.trim();
        if pansou.is_empty() {
            return "未配置 PanSou API，无法搜索换源。".to_string();
        }
        let service = SubscriptionSourceSwitchService::with_pansou_api_url(
            Arc::new(crate::clients::QuarkShareProbe::new(String::new())),
            Some(pansou.to_string()),
        );
        let candidates = match service.search_source_candidates(&sub).await {
            Ok(items) => items,
            Err(error) => return format!("换源搜索失败：{}", sanitize_error(&error.to_string())),
        };

        // 写回候选，便于后续 apply 走正式服务逻辑
        let candidates_for_store = candidates.clone();
        let _ = self
            .subscription_store
            .update(&id, |current| {
                current.source_candidates = candidates_for_store.clone();
                current.last_source_search_time = Some(unix_now());
                current.updated_at = unix_now();
            })
            .await;

        let hits = candidates
            .into_iter()
            .take(MAX_SWITCH_CANDIDATES)
            .map(|item| SwitchHit {
                candidate_id: item.id,
                note: item.note,
                url: item.url,
                score: i32::from(item.quality.score),
            })
            .collect::<Vec<_>>();
        let expires_at = unix_now() + SESSION_TTL_SECONDS;
        let session = UserSession {
            user_id,
            chat_id,
            expires_at,
            kind: SessionKind::Switch {
                subscription_id: id.clone(),
                hits: hits.clone(),
            },
        };
        self.sessions.lock().await.put(session);
        let payload = serde_json::json!({
            "subscription_id": id,
            "hits": hits,
        })
        .to_string();
        let _ = self
            .telegram_store
            .put_user_session(crate::store::telegram_bot::TelegramUserSessionRecord {
                user_id,
                chat_id,
                expires_at,
                kind: "switch".to_string(),
                payload,
            })
            .await;
        // 内联按钮
        let mut rows = Vec::new();
        let mut row = Vec::new();
        for index in 1..=hits.len() {
            row.push(json!({
                "text": format!("换源 {index}"),
                "callback_data": format!("msw:{index}")
            }));
            if row.len() == 3 {
                rows.push(std::mem::take(&mut row));
            }
        }
        if !row.is_empty() {
            rows.push(row);
        }
        if !rows.is_empty() {
            let _ = self
                .send_message_with_markup(
                    &settings,
                    chat_id,
                    "点按钮选择要应用的换源候选：",
                    Some(json!({ "inline_keyboard": rows })),
                )
                .await;
        }
        format_switch_hits(&sub, &hits)
    }

    async fn switch_apply_prepare(
        &self,
        argument: Option<&str>,
        user_id: i64,
        chat_id: i64,
    ) -> Result<PendingConfirmation, String> {
        let argument = argument
            .ok_or_else(|| "用法：/switch_apply <序号>\n先用 /switch <订阅ID>".to_string())?;
        let index = argument
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| "序号必须是从 1 开始的数字".to_string())?;
        let session = self
            .load_session(user_id, chat_id)
            .await
            .ok_or_else(|| "没有可用的换源候选，请先 /switch <订阅ID>".to_string())?;
        let SessionKind::Switch {
            subscription_id,
            hits,
        } = session.kind
        else {
            return Err("当前会话不是换源结果，请先 /switch <订阅ID>".to_string());
        };
        let hit = hits
            .get(index - 1)
            .cloned()
            .ok_or_else(|| format!("序号超出范围（1-{}）", hits.len()))?;
        // resource 格式：subscription_id|candidate_id
        let resource = format!("{}|{}", subscription_id, hit.candidate_id);
        if resource.len() > 80 {
            return Err("换源资源标识过长".to_string());
        }
        let scope = "subscriptions:write";
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let confirmation = PendingConfirmation {
            nonce: nonce.clone(),
            user_id,
            chat_id,
            action: "switch_apply".to_string(),
            scope: scope.to_string(),
            resource: resource.clone(),
            expires_at: unix_now() + CONFIRMATION_TTL_SECONDS,
            idempotency_key: format!(
                "telegram:{scope}:{user_id}:{chat_id}:switch_apply:{resource}:{nonce}"
            ),
        };
        let mut confirmations = self.confirmations.lock().await;
        let now = unix_now();
        confirmations.retain(|_, item| item.expires_at >= now);
        if confirmations.len() >= 1_000 {
            return Err("待确认操作过多，请稍后再试".to_string());
        }
        confirmations.insert(nonce, confirmation.clone());
        Ok(confirmation)
    }

    async fn execute_switch_apply(&self, resource: &str) -> Result<String, String> {
        let (subscription_id, candidate_id) = resource
            .split_once('|')
            .ok_or_else(|| "换源资源标识无效".to_string())?;
        let mut sub = self
            .subscription_store
            .get(subscription_id)
            .await
            .ok_or_else(|| "订阅不存在".to_string())?;
        let settings = self.settings_store.get().await;
        let service = SubscriptionSourceSwitchService::with_pansou_api_url(
            Arc::new(crate::clients::QuarkShareProbe::new(
                settings.quark_cookie.clone(),
            )),
            Some(settings.pansou_api_url.clone()).filter(|value| !value.trim().is_empty()),
        );
        let candidate = sub
            .source_candidates
            .iter()
            .find(|item| item.id == candidate_id)
            .cloned()
            .ok_or_else(|| "候选项不存在，请重新 /switch".to_string())?;
        let scored = service
            .probe_and_score_candidate(
                &candidate,
                &settings.quark_cookie,
                chrono::Utc::now().timestamp_millis(),
            )
            .await
            .map_err(|error| sanitize_error(&error.to_string()))?;
        if let Some(slot) = sub
            .source_candidates
            .iter_mut()
            .find(|item| item.id == scored.id)
        {
            *slot = scored.clone();
        }
        let preview = service.preview_candidate(&sub, scored, &settings, unix_now());
        if !preview.probe_ok {
            return Err(format!(
                "候选探测失败：{}",
                preview.warnings.join("；")
            ));
        }
        if !preview.season_matches {
            return Err("候选季度与订阅不匹配，禁止换源".to_string());
        }
        let reason = if preview.can_apply {
            "Telegram 用户确认后应用换源".to_string()
        } else {
            format!(
                "Telegram 用户确认后强制应用（风险：{}）",
                preview.warnings.join("；")
            )
        };
        service
            .apply_source_switch_with_audit(&mut sub, candidate_id, false, &reason)
            .map_err(|error| sanitize_error(&error.to_string()))?;
        sub.updated_at = unix_now();
        self.subscription_store
            .update(subscription_id, |current| {
                *current = sub.clone();
            })
            .await
            .map_err(|error| sanitize_error(&error.to_string()))?
            .ok_or_else(|| "订阅不存在".to_string())?;
        Ok(format!(
            "已换源：{}\n新链接：{}\n质量：{} 分\n\n建议立即 /check {}",
            sub.title,
            one_line(&sub.url, 120),
            preview.candidate.quality.score,
            short_id(&sub.id)
        ))
    }
}
