"""统一推送服务 - 支持多种推送渠道和场景"""
from __future__ import annotations

import logging
import time
from collections.abc import Callable
from datetime import datetime
from functools import wraps
from typing import Any

import requests

logger = logging.getLogger(__name__)


def retry_on_failure(max_retries: int = 3, delay: float = 1.0):
    """推送失败重试装饰器"""
    def decorator(func: Callable) -> Callable:
        @wraps(func)
        def wrapper(*args, **kwargs):
            last_error = None
            for attempt in range(max_retries):
                try:
                    return func(*args, **kwargs)
                except Exception as e:
                    last_error = e
                    if attempt < max_retries - 1:
                        logger.warning(f"{func.__name__} 第 {attempt + 1} 次尝试失败: {e}, 将在 {delay}s 后重试")
                        time.sleep(delay)
                    else:
                        logger.error(f"{func.__name__} 失败，已重试 {max_retries} 次: {e}")
            raise last_error
        return wrapper
    return decorator


class PushService:
    """统一推送服务"""

    def __init__(self, settings: dict[str, Any]):
        self.settings = settings
        self.enabled_channels = self._get_enabled_channels()

    def _get_enabled_channels(self) -> list[str]:
        """获取已启用的推送渠道"""
        channels = []
        if self.settings.get("wecom_bot_url"):
            channels.append("wecom")
        if self.settings.get("wxpusher_app_token"):
            channels.append("wxpusher")
        if self.settings.get("telegram_bot_token") and self.settings.get("telegram_chat_id"):
            channels.append("telegram")
        if self.settings.get("bark_url"):
            channels.append("bark")
        if self.settings.get("gotify_url") and self.settings.get("gotify_token"):
            channels.append("gotify")
        if self.settings.get("pushplus_token"):
            channels.append("pushplus")
        if self.settings.get("serverchan_key"):
            channels.append("serverchan")
        return channels

    def send(self, title: str, message: str, level: str = "info", silent: bool = False, scenario: str = "manual") -> dict[str, bool]:
        """发送推送到所有启用的渠道"""
        results = {}
        for channel in self.enabled_channels:
            try:
                if channel == "wecom":
                    results["wecom"] = self._send_wecom(title, message, level)
                elif channel == "wxpusher":
                    results["wxpusher"] = self._send_wxpusher(title, message)
                elif channel == "telegram":
                    results["telegram"] = self._send_telegram(title, message, level, silent)
                elif channel == "bark":
                    results["bark"] = self._send_bark(title, message, level)
                elif channel == "gotify":
                    results["gotify"] = self._send_gotify(title, message, level)
                elif channel == "pushplus":
                    results["pushplus"] = self._send_pushplus(title, message)
                elif channel == "serverchan":
                    results["serverchan"] = self._send_serverchan(title, message)
            except Exception as exc:
                logger.error(f"{channel} 推送异常: {exc}")
                results[channel] = False

        # 记录推送历史
        try:
            from .push_history_service import push_history
            push_history.add_record(title, message, self.enabled_channels, results, scenario)
        except Exception as e:
            logger.error(f"记录推送历史失败: {e}")

        return results

    @retry_on_failure(max_retries=3, delay=1.0)
    def _send_wecom(self, title: str, message: str, level: str) -> bool:
        """企业微信机器人"""
        url = self.settings.get("wecom_bot_url", "")
        if not url:
            return False
        emoji = {"info": "ℹ️", "warning": "⚠️", "error": "❌", "success": "✅"}.get(level, "ℹ️")
        payload = {
            "msgtype": "markdown",
            "markdown": {
                "content": f"### {emoji} {title}\n{message}\n> {datetime.now().strftime('%m-%d %H:%M')}"
            },
        }
        resp = requests.post(url, json=payload, timeout=10)
        return resp.json().get("errcode") == 0

    @retry_on_failure(max_retries=3, delay=1.0)
    def _send_wxpusher(self, title: str, message: str) -> bool:
        """WxPusher"""
        token = self.settings.get("wxpusher_app_token", "")
        if not token:
            return False
        uids_str = self.settings.get("wxpusher_uids", "")
        uids = [u.strip() for u in uids_str.split(",") if u.strip()] if uids_str else []
        payload = {
            "appToken": token,
            "content": f"<h3>{title}</h3><p>{message}</p>",
            "summary": title,
            "contentType": 2,
            "uids": uids,
        }
        resp = requests.post("https://wxpusher.zjiecode.com/api/send/message", json=payload, timeout=10)
        return resp.json().get("code") == 1000

    @retry_on_failure(max_retries=3, delay=1.0)
    def _send_telegram(self, title: str, message: str, level: str, silent: bool) -> bool:
        """Telegram Bot"""
        token = self.settings.get("telegram_bot_token", "")
        chat_id = self.settings.get("telegram_chat_id", "")
        if not token or not chat_id:
            return False
        emoji = {"info": "ℹ️", "warning": "⚠️", "error": "❌", "success": "✅"}.get(level, "ℹ️")
        text = f"{emoji} <b>{title}</b>\n\n{message}"
        payload = {
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML",
            "disable_notification": silent,
        }
        resp = requests.post(f"https://api.telegram.org/bot{token}/sendMessage", json=payload, timeout=10)
        return resp.json().get("ok", False)

    @retry_on_failure(max_retries=3, delay=1.0)
    def _send_bark(self, title: str, message: str, level: str) -> bool:
        """Bark (iOS)"""
        url = self.settings.get("bark_url", "")
        if not url:
            return False
        url = url.rstrip("/")
        payload = {
            "title": title,
            "body": message,
            "level": level,
            "badge": 1,
        }
        resp = requests.post(f"{url}/push", json=payload, timeout=10)
        return resp.json().get("code") == 200

    @retry_on_failure(max_retries=3, delay=1.0)
    def _send_gotify(self, title: str, message: str, level: str) -> bool:
        """Gotify"""
        url = self.settings.get("gotify_url", "").rstrip("/")
        token = self.settings.get("gotify_token", "")
        if not url or not token:
            return False
        priority = {"info": 5, "warning": 7, "error": 9, "success": 5}.get(level, 5)
        payload = {
            "title": title,
            "message": message,
            "priority": priority,
        }
        resp = requests.post(f"{url}/message?token={token}", json=payload, timeout=10)
        return resp.status_code == 200

    @retry_on_failure(max_retries=3, delay=1.0)
    def _send_pushplus(self, title: str, message: str) -> bool:
        """PushPlus"""
        token = self.settings.get("pushplus_token", "")
        if not token:
            return False
        payload = {
            "token": token,
            "title": title,
            "content": message,
            "template": "html",
        }
        resp = requests.post("http://www.pushplus.plus/send", json=payload, timeout=10)
        return resp.json().get("code") == 200

    @retry_on_failure(max_retries=3, delay=1.0)
    def _send_serverchan(self, title: str, message: str) -> bool:
        """Server酱"""
        key = self.settings.get("serverchan_key", "")
        if not key:
            return False
        payload = {
            "title": title,
            "desp": message,
        }
        resp = requests.post(f"https://sctapi.ftqq.com/{key}.send", data=payload, timeout=10)
        return resp.json().get("code") == 0


# 推送场景模板
class PushScenarios:
    """推送场景和消息模板"""

    @staticmethod
    def subscription_update(sub_title: str, new_items: list) -> tuple[str, str, str, str]:
        """订阅更新"""
        count = len(new_items)
        items_text = "\n".join([f"• {item.get('title', '未知')}" for item in new_items[:5]])
        if count > 5:
            items_text += f"\n... 还有 {count - 5} 项"
        return (
            f"📺 订阅更新：{sub_title}",
            f"发现 {count} 个新资源：\n\n{items_text}",
            "info",
            "subscription_update"
        )

    @staticmethod
    def subscription_failed(sub_title: str, reason: str) -> tuple[str, str, str, str]:
        """订阅失败"""
        return (
            f"❌ 订阅失效：{sub_title}",
            f"原因：{reason}\n请检查链接或重新创建订阅",
            "error",
            "subscription_failed"
        )

    @staticmethod
    def subscription_completed(sub_title: str) -> tuple[str, str, str, str]:
        """订阅完成"""
        return (
            f"✅ 订阅完结：{sub_title}",
            "该订阅已标记为完结，不再自动检查更新",
            "success",
            "subscription_completed"
        )

    @staticmethod
    def download_completed(item_title: str) -> tuple[str, str, str, str]:
        """下载完成"""
        return (
            "⬇️ 下载完成",
            f"已完成：{item_title}",
            "success",
            "download_completed"
        )

    @staticmethod
    def save_completed(sub_title: str, count: int) -> tuple[str, str, str, str]:
        """转存完成"""
        return (
            f"💾 转存完成：{sub_title}",
            f"已转存 {count} 个文件到夸克网盘",
            "success",
            "save_completed"
        )

    @staticmethod
    def save_failed(sub_title: str, reason: str) -> tuple[str, str, str, str]:
        """转存失败"""
        return (
            f"⚠️ 转存失败：{sub_title}",
            f"原因：{reason}",
            "warning",
            "save_failed"
        )

    @staticmethod
    def daily_summary(total_subs: int, active_subs: int, new_items: int) -> tuple[str, str, str]:
        """每日摘要"""
        return (
            "📊 每日订阅摘要",
            f"总订阅：{total_subs} 个\n活跃订阅：{active_subs} 个\n新增资源：{new_items} 项",
            "info"
        )


def get_push_service(settings: dict[str, Any]) -> PushService:
    """获取推送服务实例"""
    return PushService(settings)
