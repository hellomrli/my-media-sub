"""每日摘要推送服务"""
from __future__ import annotations

import logging
from datetime import datetime, timedelta
from typing import Any

from ..stores.subscription_store import subscription_store
from .push_service import get_push_service

logger = logging.getLogger(__name__)


def generate_daily_summary(settings: dict[str, Any]) -> dict[str, Any]:
    """生成每日摘要"""
    # 获取昨天的日期范围
    today = datetime.now().replace(hour=0, minute=0, second=0, microsecond=0)
    yesterday = today - timedelta(days=1)
    
    subscriptions = subscription_store.list()
    
    # 统计数据
    total_subs = len(subscriptions)
    active_subs = sum(1 for s in subscriptions if not s.get("completed"))
    new_items_count = 0
    updated_subs = []
    
    # 统计昨天的更新
    for sub in subscriptions:
        items = sub.get("items", [])
        # 简单计数：假设最近的items可能是昨天添加的
        # 实际应该检查 item 的添加时间戳
        if items:
            # 这里简化处理，实际应该有时间戳
            new_count = len([i for i in items if i.get("new", False)])
            if new_count > 0:
                new_items_count += new_count
                updated_subs.append({
                    "title": sub.get("title", "未知"),
                    "count": new_count
                })
    
    summary = {
        "date": yesterday.strftime("%Y-%m-%d"),
        "total_subscriptions": total_subs,
        "active_subscriptions": active_subs,
        "new_items_count": new_items_count,
        "updated_subscriptions": len(updated_subs),
        "updates": updated_subs[:10],  # 最多显示10个
    }
    
    return summary


def send_daily_summary(settings: dict[str, Any]) -> dict[str, Any]:
    """发送每日摘要推送"""
    try:
        summary = generate_daily_summary(settings)
        
        if summary["new_items_count"] == 0:
            logger.info("昨日无新增资源，跳过每日摘要推送")
            return {"ok": True, "message": "无新增资源", "skipped": True}
        
        # 构建消息
        title = f"📊 每日摘要 ({summary['date']})"
        
        message_lines = [
            f"订阅总数：{summary['total_subscriptions']} (活跃 {summary['active_subscriptions']})",
            f"新增资源：{summary['new_items_count']} 项",
            f"更新订阅：{summary['updated_subscriptions']} 个",
        ]
        
        if summary["updates"]:
            message_lines.append("\n📺 更新列表：")
            for upd in summary["updates"]:
                message_lines.append(f"• {upd['title']} (+{upd['count']})")
        
        message = "\n".join(message_lines)
        
        # 发送推送
        push_service = get_push_service(settings)
        results = push_service.send(title, message, "info", scenario="daily_summary")
        
        success_count = sum(1 for v in results.values() if v)
        
        return {
            "ok": success_count > 0,
            "message": f"每日摘要已发送到 {success_count} 个渠道",
            "summary": summary,
            "results": results,
        }
    except Exception as e:
        logger.error(f"每日摘要推送失败: {e}")
        return {"ok": False, "message": str(e)}
