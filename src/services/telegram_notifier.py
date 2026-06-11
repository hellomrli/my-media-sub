from __future__ import annotations

import asyncio
import logging
from typing import Any

from telegram import Bot
from telegram.error import TelegramError

from ..config_new import settings

logger = logging.getLogger(__name__)


class TelegramNotifier:
    """Telegram notification service."""
    
    def __init__(self):
        self.bot: Bot | None = None
        self.chat_id = settings.telegram_chat_id
        self.enabled = bool(settings.telegram_bot_token and settings.telegram_chat_id)
        
        if self.enabled:
            self.bot = Bot(token=settings.telegram_bot_token)
    
    async def send_notification(self, title: str, message: str, level: str = "info"):
        """Send notification to Telegram."""
        if not self.enabled or not self.bot:
            logger.debug(f"Telegram not enabled, skipping notification: {title}")
            return
        
        # Check if level meets threshold
        level_order = ["info", "success", "warning", "error"]
        threshold_order = ["info", "success", "warning", "error"]
        
        try:
            threshold_idx = threshold_order.index(settings.notification_level)
            current_idx = level_order.index(level)
            if current_idx < threshold_idx:
                logger.debug(f"Notification level {level} below threshold {settings.notification_level}")
                return
        except ValueError:
            pass
        
        # Format message with emoji
        emoji_map = {
            "info": "ℹ️",
            "success": "✅",
            "warning": "⚠️",
            "error": "❌",
        }
        emoji = emoji_map.get(level, "📢")
        
        full_message = f"{emoji} <b>{title}</b>\n\n{message}"
        
        try:
            await self.bot.send_message(
                chat_id=self.chat_id,
                text=full_message,
                parse_mode="HTML",
            )
            logger.info(f"Telegram notification sent: {title}")
        except TelegramError as e:
            logger.error(f"Failed to send Telegram notification: {e}")
    
    async def send_subscription_update(self, subscription_title: str, new_files: list[str]):
        """Send subscription update notification."""
        if not new_files:
            return
        
        files_text = "\n".join(f"• {f}" for f in new_files[:10])
        if len(new_files) > 10:
            files_text += f"\n... 还有 {len(new_files) - 10} 个文件"
        
        message = f"订阅《{subscription_title}》发现 {len(new_files)} 个新文件：\n\n{files_text}"
        await self.send_notification("订阅更新", message, "success")
    
    async def send_subscription_failed(self, subscription_title: str, error: str):
        """Send subscription check failed notification."""
        message = f"订阅《{subscription_title}》检查失败：\n\n{error}"
        await self.send_notification("订阅检查失败", message, "error")
    
    async def send_subscription_completed(self, subscription_title: str):
        """Send subscription completed notification."""
        message = f"订阅《{subscription_title}》已自动标记为完结（连续多次检查无新增内容）"
        await self.send_notification("订阅完结", message, "info")


# Global notifier instance
telegram_notifier = TelegramNotifier()
