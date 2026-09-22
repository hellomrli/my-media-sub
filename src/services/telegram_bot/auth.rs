//! `telegram_bot` 的auth相关逻辑。
//!
//! 从母线 `telegram_bot.rs` 拆出（该文件原有 1914 行、单个 `impl` 块 1197 行，
//! 审查成本很高）。本模块只放**无副作用**的纯函数，因此可以独立阅读与测试。
//!
//! 可见性统一用 `pub(super)`：父模块通过 `use auth::*;` 引入，这样
//! `include!` 进来的 `commands.rs` / `menus.rs`（它们文本上位于父模块内）
//! 也能照常调用，无需改动任何调用点。

use crate::models::Settings;

use super::TelegramChat;
pub(super) fn valid_common_config(settings: &Settings) -> bool {
    !settings.telegram_bot_token.trim().is_empty()
        && !settings.telegram_bot_allowed_user_ids.is_empty()
        && !effective_allowed_chats(settings).is_empty()
}
pub(super) fn valid_webhook_config(settings: &Settings) -> bool {
    valid_common_config(settings)
        && settings
            .telegram_bot_webhook_public_url
            .starts_with("https://")
        && settings.telegram_bot_webhook_path_secret.len() >= 24
        && settings.telegram_bot_webhook_secret.len() >= 24
}
pub(super) fn effective_allowed_chats(settings: &Settings) -> Vec<i64> {
    let mut chats = settings.telegram_bot_allowed_chat_ids.clone();
    if let Ok(chat_id) = settings.telegram_chat_id.trim().parse::<i64>() {
        if !chats.contains(&chat_id) {
            chats.push(chat_id);
        }
    }
    chats
}
pub(super) fn is_authorized(settings: &Settings, user_id: i64, chat: &TelegramChat) -> bool {
    settings.telegram_bot_allowed_user_ids.contains(&user_id)
        && effective_allowed_chats(settings).contains(&chat.id)
        && (!settings.telegram_bot_private_only || chat.kind == "private")
}
