from __future__ import annotations

from fastapi import APIRouter, Depends

from ..auth import require_auth
from ..services.push_test_service import test_all_push_channels, test_push_channel
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
