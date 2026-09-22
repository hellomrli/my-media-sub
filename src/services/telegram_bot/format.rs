//! `telegram_bot` 的format相关逻辑。
//!
//! 从母线 `telegram_bot.rs` 拆出（该文件原有 1914 行、单个 `impl` 块 1197 行，
//! 审查成本很高）。本模块只放**无副作用**的纯函数，因此可以独立阅读与测试。
//!
//! 可见性统一用 `pub(super)`：父模块通过 `use format::*;` 引入，这样
//! `include!` 进来的 `commands.rs` / `menus.rs`（它们文本上位于父模块内）
//! 也能照常调用，无需改动任何调用点。

use serde_json::json;

use crate::jobs::{JobErrorClass, JobStatus};
use crate::models::Subscription;
use crate::services::media_calendar::shanghai_offset;

use super::{LIST_PAGE_SIZE, TELEGRAM_MESSAGE_LIMIT};
pub(super) fn resolve_unique_id<'a>(
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
        [] => Err(format!("{label}不存在：{}", tg_escape(value))),
        _ => Err(format!("{label} ID 前缀不唯一，请提供更长 ID")),
    }
}
pub(super) fn page_bounds(total: usize, requested_page: usize) -> (usize, usize, usize, usize) {
    let pages = total.max(1).div_ceil(LIST_PAGE_SIZE);
    let page = requested_page.clamp(1, pages);
    let start = ((page - 1) * LIST_PAGE_SIZE).min(total);
    let end = (start + LIST_PAGE_SIZE).min(total);
    (start, end, page, pages)
}
pub(super) fn split_message(value: &str) -> Vec<String> {
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
pub(super) fn chunk_chars(value: &str, limit: usize) -> Vec<String> {
    let chars = value.chars().collect::<Vec<_>>();
    chars
        .chunks(limit)
        .map(|chunk| chunk.iter().collect())
        .collect()
}
pub(super) fn help_text() -> &'static str {
    "🤖 <b>MEDIA/SUB 控制</b>

直接发送中文也可唤起：状态 / 订阅 / 任务 / 日历 / 通知 / 诊断 / 帮助 / 检查全部
🎬 直接发送豆瓣链接，即可搜索链接对应的剧集

<b>资源</b>
/search &lt;关键词&gt; — 搜索夸克资源
/subscribe &lt;序号&gt; [季号] — 订阅搜索结果（需确认）
/transfer &lt;序号&gt; — 转存搜索结果到网盘（需确认，完成后可一键下载）
/switch &lt;订阅ID&gt; — 搜索换源候选
/switch_apply &lt;序号&gt; — 应用换源（需确认）

<b>只读</b>
/status — 系统概况
/subscriptions [页码] — 订阅列表
/subscription &lt;ID&gt; — 订阅详情
/calendar [页码] — 本周排期
/jobs [页码] — 最近任务
/job &lt;ID&gt; — 任务详情
/notifications [页码] — 未读通知

/diagnostics — Bot 诊断

<b>受控写（需确认）</b>
/check &lt;订阅ID|all&gt;
/retry &lt;Job ID&gt;
/cancel &lt;Job ID&gt;
/signin
/read &lt;通知ID|all&gt;"
}
pub(super) fn status_name(status: &JobStatus) -> &'static str {
    match status {
        JobStatus::Queued => "⏳ 排队",
        JobStatus::Running => "🔄 运行",
        JobStatus::Succeeded => "✅ 成功",
        JobStatus::Failed => "❌ 失败",
        JobStatus::Canceled => "🚫 取消",
    }
}
pub(super) fn subscription_state_label(item: &Subscription) -> &'static str {
    if !item.enabled {
        "⏸ 停用"
    } else if item.completed {
        "🏁 完成"
    } else {
        "✅ 启用"
    }
}
pub(super) fn notification_level_label(level: &str) -> String {
    match level {
        "info" => "ℹ️ 信息".to_string(),
        "success" => "✅ 成功".to_string(),
        "warning" => "⚠️ 警告".to_string(),
        "error" => "❌ 错误".to_string(),
        other => tg_escape(other),
    }
}
pub(super) fn calendar_status_label(status: &str) -> String {
    match status {
        "today" => "🟢 今日更新".to_string(),
        "this_week" => "🟡 本周待更新".to_string(),
        "aired_undiscovered" => "🔍 已播未发现".to_string(),
        "discovered_pending_transfer" => "📥 待转存".to_string(),
        "transferred_pending_download" => "⬇️ 待下载".to_string(),
        "completed_missing" => "⚠️ 完结缺集".to_string(),
        "ready" => "✅ 已就绪".to_string(),
        "scheduled" => "📅 已排期".to_string(),
        "unknown_schedule" => "❓ 排期未知".to_string(),
        other => tg_escape(other),
    }
}
pub(super) fn job_error_class_label(class: &JobErrorClass) -> &'static str {
    match class {
        JobErrorClass::RateLimited => "限流",
        JobErrorClass::Transient => "瞬时故障",
        JobErrorClass::Authentication => "认证失败",
        JobErrorClass::Validation => "参数或规则错误",
        JobErrorClass::NotFound => "资源不存在",
        JobErrorClass::Permanent => "永久错误",
        JobErrorClass::Internal => "内部错误",
        JobErrorClass::TimedOut => "超时",
    }
}
/// Telegram HTML parse_mode 下的用户内容转义。所有来自 Store、搜索结果或
/// 上游错误的信息都必须先经过这里，否则非法标签会让整条消息发送失败。
pub(super) fn tg_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
/// Telegram 对非法 HTML 的典型拒绝原因；命中时去掉 parse_mode 重发，
/// 保证用户内容异常不会让整条消息发送失败。
pub(super) fn telegram_parse_error(error: &str) -> bool {
    let lower = error.to_lowercase();
    lower.contains("parse entities") || lower.contains("entity")
}
/// 只读列表的翻页按钮；单页时返回 None（不附加键盘）。
pub(super) fn list_page_markup(
    command: &str,
    page: usize,
    pages: usize,
) -> Option<serde_json::Value> {
    if pages <= 1 {
        return None;
    }
    let mut row = Vec::new();
    if page > 1 {
        row.push(json!({
            "text": "◀️ 上一页",
            "callback_data": format!("page:{command}:{}", page - 1)
        }));
    }
    if page < pages {
        row.push(json!({
            "text": "下一页 ▶️",
            "callback_data": format!("page:{command}:{}", page + 1)
        }));
    }
    (!row.is_empty()).then(|| json!({ "inline_keyboard": [row] }))
}
/// 解析分页 Callback：`page:<command>:<页码>`，只放行白名单列表命令。
pub(super) fn parse_page_callback(data: &str) -> Option<(&'static str, usize)> {
    let rest = data.strip_prefix("page:")?;
    let (command, page_text) = rest.split_once(':')?;
    let command = match command {
        "subscriptions" => "subscriptions",
        "jobs" => "jobs",
        "notifications" => "notifications",
        "calendar" => "calendar",
        _ => return None,
    };
    let page = page_text.parse::<usize>().ok()?.max(1);
    Some((command, page))
}
pub(super) fn short_id(value: &str) -> &str {
    value.get(..value.len().min(8)).unwrap_or(value)
}
pub(super) fn one_line(value: &str, limit: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() > limit {
        format!("{}…", normalized.chars().take(limit).collect::<String>())
    } else {
        normalized
    }
}
pub(super) fn timestamp_text(value: Option<i64>) -> String {
    value
        .and_then(|timestamp| chrono::DateTime::from_timestamp(timestamp, 0))
        .map(|date| {
            date.with_timezone(&shanghai_offset())
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| "无".to_string())
}
