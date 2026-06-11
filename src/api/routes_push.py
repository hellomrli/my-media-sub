from __future__ import annotations

from fastapi import APIRouter, Depends

from ..auth import require_auth
from ..services.push_test_service import test_all_push_channels, test_push_channel
from ..services.push_history_service import push_history
from ..services.daily_summary_service import send_daily_summary, generate_daily_summary
from ..stores.settings_store import settings_store

router = APIRouter(dependencies=[Depends(require_auth)])


@router.post("/api/push/test")
def test_all_channels():
    """测试所有推送渠道"""
    settings = settings_store.get()
    return test_all_push_channels(settings)


@router.post("/api/push/test/{channel}")
def test_single_channel(channel: str):
    """测试单个推送渠道"""
    settings = settings_store.get()
    return test_push_channel(settings, channel)


@router.get("/api/push/history")
def get_push_history(limit: int = 50):
    """获取推送历史"""
    return {"records": push_history.get_recent(limit)}


@router.get("/api/push/stats")
def get_push_stats():
    """获取推送统计"""
    return push_history.get_stats()


@router.get("/api/push/daily-summary")
def get_daily_summary():
    """获取每日摘要数据"""
    settings = settings_store.get()
    return generate_daily_summary(settings)


@router.post("/api/push/daily-summary")
def send_daily_summary_now():
    """立即发送每日摘要推送"""
    settings = settings_store.get()
    return send_daily_summary(settings)
