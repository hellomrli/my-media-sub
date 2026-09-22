//! `telegram_bot` 的callback_sign相关逻辑。
//!
//! 从母线 `telegram_bot.rs` 拆出（该文件原有 1914 行、单个 `impl` 块 1197 行，
//! 审查成本很高）。本模块只放**无副作用**的纯函数，因此可以独立阅读与测试。
//!
//! 可见性统一用 `pub(super)`：父模块通过 `use callback_sign::*;` 引入，这样
//! `include!` 进来的 `commands.rs` / `menus.rs`（它们文本上位于父模块内）
//! 也能照常调用，无需改动任何调用点。

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

use crate::models::Settings;
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
        "download" => "d",
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
    // 用 first() 而不是 [0]：Settings 是用户可写 JSON，一旦上面的
    // `.len() != 1` 将来放宽为 `> 1`（多管理员），索引写法会立刻 panic。
    let user_id = *settings.telegram_bot_allowed_user_ids.first()?;
    let expires = to_base36(expires_at.max(0) as u64);
    let material = format!("{code}|{resource}|{expires}|{user_id}|{chat_id}");
    let signature = telegram_callback_signature(settings, &material)?;
    let data = format!("prompt:{code}.{resource}.{expires}.{signature}");
    (data.len() <= 64).then_some(data)
}
pub(super) fn verify_prompt_callback_data(
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
        "d" => "download",
        _ => return Err("按钮动作不受支持".to_string()),
    };
    Ok((action.to_string(), resource.to_string()))
}
pub(super) fn telegram_callback_signature(settings: &Settings, material: &str) -> Option<String> {
    // 签名密钥优先使用 webhook secret；未配置（纯 long_polling 部署）时
    // 回落到 Bot Token 派生密钥。否则「继续下载/查看详情」等主动按钮在
    // 没有 webhook secret 的部署里会静默消失。Bot Token 本就是服务端
    // 保密材料，只作为 HMAC 密钥参与签名，不产生额外暴露面。
    let secret = match settings.telegram_bot_webhook_secret.trim() {
        value if value.len() >= 24 => value.as_bytes().to_vec(),
        _ => {
            let token = settings.telegram_bot_token.trim();
            if token.is_empty() {
                return None;
            }
            format!("my-media-sub:telegram-callback:{token}").into_bytes()
        }
    };
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &secret);
    let signature = URL_SAFE_NO_PAD.encode(ring::hmac::sign(&key, material.as_bytes()).as_ref());
    Some(signature.chars().take(8).collect())
}
pub(super) fn to_base36(mut value: u64) -> String {
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
pub(super) fn from_base36(value: &str) -> Option<u64> {
    value.chars().try_fold(0_u64, |result, character| {
        character
            .to_digit(36)
            .map(|digit| result.saturating_mul(36).saturating_add(u64::from(digit)))
    })
}
