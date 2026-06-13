from __future__ import annotations

import asyncio
import contextlib
import logging
from typing import Any

from ..stores.notification_store import notification_store
from ..stores.settings_store import settings_store
from .subscription_service import check_all_subscriptions

logger = logging.getLogger(__name__)

_scheduler_task: asyncio.Task | None = None


def scheduler_state() -> dict[str, Any]:
    settings = settings_store.get()
    return {
        "enabled": bool(settings.get("subscription_scheduler_enabled")),
        "interval_minutes": settings.get("subscription_check_interval_minutes"),
        "running": _scheduler_task is not None and not _scheduler_task.done(),
    }


def _interval_seconds() -> int:
    settings = settings_store.get()
    minutes = settings.get("subscription_check_interval_minutes") or 60
    try:
        minutes = int(minutes)
    except (TypeError, ValueError):
        minutes = 60
    return max(minutes, 5) * 60


async def _run_scheduler() -> None:
    while True:
        settings = settings_store.get()
        if settings.get("subscription_scheduler_enabled"):
            try:
                await asyncio.to_thread(check_all_subscriptions)
            except Exception as exc:
                logger.exception("subscription scheduler check failed")
                notification_store.add(
                    "warning",
                    "subscription_scheduler_failed",
                    "订阅定时检查失败",
                    str(exc),
                    {},
                )
        await asyncio.sleep(_interval_seconds())


def start_scheduler() -> None:
    global _scheduler_task
    if _scheduler_task is not None and not _scheduler_task.done():
        return
    _scheduler_task = asyncio.create_task(_run_scheduler())


async def stop_scheduler() -> None:
    global _scheduler_task
    if _scheduler_task is None:
        return
    _scheduler_task.cancel()
    with contextlib.suppress(asyncio.CancelledError):
        await _scheduler_task
    _scheduler_task = None
