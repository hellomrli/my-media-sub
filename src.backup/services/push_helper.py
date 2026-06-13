"""推送服务辅助函数 - 提供同步和异步接口"""
from __future__ import annotations

import asyncio
import logging
from typing import Any

logger = logging.getLogger(__name__)


def send_push_background(
    settings: dict[str, Any],
    title: str,
    message: str,
    level: str = "info",
    silent: bool = False,
    scenario: str = "manual"
) -> None:
    """在后台异步发送推送（非阻塞）"""
    try:
        # 尝试获取当前事件循环
        loop = asyncio.get_event_loop()
        if loop.is_running():
            # 如果循环正在运行，创建后台任务
            asyncio.create_task(_async_send_push(settings, title, message, level, silent, scenario))
        else:
            # 如果没有运行的循环，使用 run_until_complete
            loop.run_until_complete(_async_send_push(settings, title, message, level, silent, scenario))
    except RuntimeError:
        # 如果没有事件循环，创建新的
        asyncio.run(_async_send_push(settings, title, message, level, silent, scenario))
    except Exception as e:
        logger.error(f"后台推送失败: {e}")


async def _async_send_push(
    settings: dict[str, Any],
    title: str,
    message: str,
    level: str,
    silent: bool,
    scenario: str
) -> None:
    """内部异步推送函数"""
    try:
        from .push_service_async import AsyncPushService
        async with AsyncPushService(settings) as push_service:
            await push_service.send(title, message, level, silent, scenario)
    except Exception as e:
        logger.error(f"异步推送失败: {e}")
        # 降级到同步推送
        try:
            from .push_service import get_push_service
            push_service = get_push_service(settings)
            push_service.send(title, message, level, silent, scenario)
        except Exception as e2:
            logger.error(f"同步推送降级也失败: {e2}")


def send_push_sync(
    settings: dict[str, Any],
    title: str,
    message: str,
    level: str = "info",
    silent: bool = False,
    scenario: str = "manual"
) -> dict[str, bool]:
    """同步发送推送（阻塞）"""
    from .push_service import get_push_service
    push_service = get_push_service(settings)
    return push_service.send(title, message, level, silent, scenario)


async def send_push_async(
    settings: dict[str, Any],
    title: str,
    message: str,
    level: str = "info",
    silent: bool = False,
    scenario: str = "manual"
) -> dict[str, bool]:
    """异步发送推送"""
    from .push_service_async import AsyncPushService
    async with AsyncPushService(settings) as push_service:
        return await push_service.send(title, message, level, silent, scenario)
