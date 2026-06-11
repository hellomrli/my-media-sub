"""WeChat notification delivery channel.

Supported platforms:
- wecom_bot: 企业微信群机器人 webhook
- wxpusher: WxPusher 推送服务
"""
from __future__ import annotations

import logging
from typing import Any

import requests

logger = logging.getLogger(__name__)


def send_wecom_bot(webhook_url: str, title: str, message: str, level: str = "info") -> bool:
    """Send a notification via 企业微信机器人 webhook.
    
    Supports text and markdown formats. Uses markdown for rich formatting
    when level is warning/error to highlight.
    """
    if not webhook_url:
        return False
    
    emoji = {"info": "ℹ️", "warning": "⚠️", "error": "❌"}.get(level, "ℹ️")
    
    payload = {
        "msgtype": "markdown",
        "markdown": {
            "content": (
                f"### {emoji} {title}\n"
                f"{message}\n"
                f"> 来自 Lain 的媒体订阅 · {__import__('time').strftime('%m-%d %H:%M')}"
            )
        },
    }
    
    try:
        resp = requests.post(webhook_url, json=payload, timeout=10)
        data = resp.json()
        if data.get("errcode") == 0:
            return True
        logger.warning("企业微信推送失败: %s", data.get("errmsg", resp.text))
        return False
    except Exception as exc:
        logger.warning("企业微信推送异常: %s", exc)
        return False


def send_wxpusher(app_token: str, title: str, message: str, uids: list[str] | None = None, topic_ids: list[int] | None = None) -> bool:
    """Send a notification via WxPusher.
    
    Args:
        app_token: WxPusher application token
        title: Notification title
        message: Notification body (supports HTML)
        uids: Target user UIDs (optional, sends to all followers if empty)
        topic_ids: Target topic IDs (optional)
    """
    if not app_token:
        return False
    
    payload = {
        "appToken": app_token,
        "content": f"<h3>{title}</h3><p>{message}</p>",
        "summary": title,
        "contentType": 2,  # HTML
        "uids": uids or [],
        "topicIds": topic_ids or [],
    }
    
    try:
        resp = requests.post(
            "https://wxpusher.zjiecode.com/api/send/message",
            json=payload,
            timeout=10,
        )
        data = resp.json()
        if data.get("code") == 1000:
            return True
        logger.warning("WxPusher 推送失败: %s", data.get("msg", resp.text))
        return False
    except Exception as exc:
        logger.warning("WxPusher 推送异常: %s", exc)
        return False


def send_notification(settings: dict[str, Any], level: str, title: str, message: str) -> None:
    """Route a notification to all configured WeChat channels."""
    # 企业微信机器人
    wecom_url = settings.get("wecom_bot_url", "")
    if wecom_url:
        send_wecom_bot(wecom_url, title, message, level)
    
    # WxPusher
    wxpusher_token = settings.get("wxpusher_app_token", "")
    if wxpusher_token:
        uids_str = settings.get("wxpusher_uids", "")
        uids = [u.strip() for u in uids_str.split(",") if u.strip()] if uids_str else None
        send_wxpusher(wxpusher_token, title, message, uids=uids)
