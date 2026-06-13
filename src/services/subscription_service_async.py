from __future__ import annotations

import asyncio
import logging
from typing import Any

from sqlalchemy import select

from ..clients.quark_async import QuarkShareProbeAsync
from ..database import Notification, Subscription, async_session
from ..task_queue import task_queue
from .auto_completion import check_auto_completion, mark_completed, update_no_update_count
from .telegram_notifier import telegram_notifier

logger = logging.getLogger(__name__)


async def create_subscription(
    keyword: str,
    url: str,
    password: str,
    media_type: str,
    notify_only: bool,
    rules: dict[str, Any] | None = None,
) -> Subscription:
    """Create a new subscription."""
    import uuid

    sub = Subscription(
        id=str(uuid.uuid4()),
        keyword=keyword,
        url=url,
        password=password or "",
        media_type=media_type,
        notify_only=notify_only,
        enabled=True,
        rules=rules or {},
    )

    async with async_session() as session:
        session.add(sub)
        await session.commit()
        await session.refresh(sub)

    # Trigger initial check in background
    await task_queue.put(
        f"check_subscription_{sub.id}",
        check_subscription(sub.id),
        priority=1,
    )

    logger.info(f"Created subscription {sub.id} for {keyword}")
    return sub


async def check_subscription(subscription_id: str) -> dict[str, Any]:
    """Check a subscription for updates."""
    logger.info(f"Checking subscription {subscription_id}")

    async with async_session() as session:
        result = await session.execute(
            select(Subscription).where(Subscription.id == subscription_id)
        )
        sub = result.scalar_one_or_none()

        if not sub:
            logger.error(f"Subscription {subscription_id} not found")
            return {"error": "Subscription not found"}

        if not sub.enabled or sub.completed:
            logger.info(f"Subscription {subscription_id} disabled or completed, skipping")
            return {"skipped": True}

        # Probe share
        probe = QuarkShareProbeAsync(cookie="")  # Use global cookie from settings
        probe_result = await probe.probe(sub.url, sub.password)

        # Update probe result
        from datetime import datetime
        sub.last_check_time = datetime.now()
        sub.last_probe = {
            "ok": probe_result.ok,
            "state": probe_result.state,
            "message": probe_result.message,
            "files": probe_result.files[:100],  # Limit stored files
            "file_count": probe_result.file_count,
            "episode_count": probe_result.episode_count,
        }

        if not probe_result.ok:
            # Create notification for failed check
            notification = Notification(
                level="error",
                title=f"订阅检查失败：{sub.keyword}",
                message=f"链接状态：{probe_result.state}\n错误：{probe_result.message}",
                subscription_id=sub.id,
            )
            session.add(notification)
            await session.commit()

            await telegram_notifier.send_subscription_failed(sub.keyword, probe_result.message)
            return {"error": probe_result.message}

        # Find new files
        saved_files = set(sub.saved_files or [])
        current_files = {f["name"]: f for f in probe_result.files}
        new_files = [name for name in current_files if name not in saved_files]

        # Apply filters from rules
        rules = sub.rules or {}
        filtered_new = apply_subscription_rules(new_files, rules)

        # Update no-update counter
        await update_no_update_count(sub.id, len(filtered_new) > 0)

        if filtered_new:
            # Create notification
            notification = Notification(
                level="success",
                title=f"订阅更新：{sub.keyword}",
                message=f"发现 {len(filtered_new)} 个新文件",
                subscription_id=sub.id,
            )
            session.add(notification)

            # Update saved files
            sub.saved_files = list(saved_files | set(filtered_new))

            await session.commit()

            # Send Telegram notification
            await telegram_notifier.send_subscription_update(sub.keyword, filtered_new)

            logger.info(f"Subscription {sub.id} found {len(filtered_new)} new files")
        else:
            await session.commit()
            logger.info(f"Subscription {sub.id} no new files")

        # Check auto-completion
        should_complete, reason = await check_auto_completion(sub.id)
        if should_complete and reason:
            await mark_completed(sub.id, reason)
            await telegram_notifier.send_subscription_completed(sub.keyword)

        return {
            "subscription_id": sub.id,
            "new_files": filtered_new,
            "total_files": probe_result.file_count,
        }


async def check_all_subscriptions() -> list[dict[str, Any]]:
    """Check all active subscriptions concurrently."""
    async with async_session() as session:
        result = await session.execute(
            select(Subscription).where(
                Subscription.enabled.is_(True),
                Subscription.completed.is_(False),
            )
        )
        subscriptions = result.scalars().all()

    if not subscriptions:
        logger.info("No active subscriptions to check")
        return []

    logger.info(f"Checking {len(subscriptions)} subscriptions concurrently")

    tasks = [check_subscription(sub.id) for sub in subscriptions]
    results = await asyncio.gather(*tasks, return_exceptions=True)

    # Filter out exceptions and return results
    valid_results = []
    for result in results:
        if isinstance(result, Exception):
            logger.error(f"Subscription check failed: {result}", exc_info=result)
        else:
            valid_results.append(result)

    return valid_results


def apply_subscription_rules(files: list[str], rules: dict[str, Any]) -> list[str]:
    """Apply subscription rules to filter files."""
    if not files:
        return []

    # Include keywords
    include = rules.get("include_keywords", [])
    if include:
        files = [f for f in files if any(kw.lower() in f.lower() for kw in include)]

    # Exclude keywords
    exclude = rules.get("exclude_keywords", [])
    if exclude:
        files = [f for f in files if not any(kw.lower() in f.lower() for kw in exclude)]

    # Only latest (for series)
    if rules.get("only_latest", False) and files:
        files = [files[-1]]

    return files
