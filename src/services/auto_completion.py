from __future__ import annotations

from typing import Any
import logging
from datetime import datetime

from ..config_new import settings
from ..database import async_session, Subscription
from sqlalchemy import select

logger = logging.getLogger(__name__)


async def check_auto_completion(subscription_id: str) -> tuple[bool, str | None]:
    """
    Check if subscription should be auto-completed.
    
    Returns (should_complete, reason)
    """
    async with async_session() as session:
        result = await session.execute(
            select(Subscription).where(Subscription.id == subscription_id)
        )
        sub = result.scalar_one_or_none()
        
        if not sub:
            return False, None
        
        if sub.completed:
            return False, None
        
        # Check 1: No updates for N consecutive checks
        threshold = settings.auto_complete_after_no_updates
        if sub.no_update_count >= threshold:
            return True, f"连续 {threshold} 次检查无新增内容"
        
        # Check 2: Episode count matches expected (if configured)
        rules = sub.rules or {}
        expected_episodes = rules.get("expected_episodes")
        if expected_episodes:
            last_probe = sub.last_probe or {}
            current_episodes = last_probe.get("episode_count", 0)
            if current_episodes >= expected_episodes:
                return True, f"剧集数已达到预期 ({current_episodes}/{expected_episodes})"
        
        return False, None


async def mark_completed(subscription_id: str, reason: str):
    """Mark subscription as completed."""
    async with async_session() as session:
        result = await session.execute(
            select(Subscription).where(Subscription.id == subscription_id)
        )
        sub = result.scalar_one_or_none()
        
        if not sub:
            logger.error(f"Subscription {subscription_id} not found")
            return
        
        sub.completed = True
        check_history = sub.check_history or []
        check_history.append({
            "action": "auto_completed",
            "reason": reason,
            "timestamp": str(datetime.now()),
        })
        sub.check_history = check_history
        
        await session.commit()
        logger.info(f"Subscription {subscription_id} auto-completed: {reason}")


async def update_no_update_count(subscription_id: str, has_new_files: bool):
    """Update the no-update counter."""
    async with async_session() as session:
        result = await session.execute(
            select(Subscription).where(Subscription.id == subscription_id)
        )
        sub = result.scalar_one_or_none()
        
        if not sub:
            return
        
        if has_new_files:
            sub.no_update_count = 0
        else:
            sub.no_update_count = (sub.no_update_count or 0) + 1
        
        await session.commit()
