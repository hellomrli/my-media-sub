"""推送测试服务"""
from __future__ import annotations

from typing import Any

from .push_helper import send_push_sync


def test_push_channel(settings: dict[str, Any], channel: str) -> dict[str, Any]:
    """测试单个推送渠道"""
    # 检查渠道是否配置
    channel_checks = {
        "wecom": lambda: bool(settings.get("wecom_bot_url")),
        "wxpusher": lambda: bool(settings.get("wxpusher_app_token")),
        "telegram": lambda: bool(settings.get("telegram_bot_token") and settings.get("telegram_chat_id")),
        "bark": lambda: bool(settings.get("bark_url")),
        "gotify": lambda: bool(settings.get("gotify_url") and settings.get("gotify_token")),
        "pushplus": lambda: bool(settings.get("pushplus_token")),
        "serverchan": lambda: bool(settings.get("serverchan_key")),
    }
    
    if channel not in channel_checks:
        return {"ok": False, "message": f"未知推送渠道：{channel}"}
    
    if not channel_checks[channel]():
        return {"ok": False, "message": f"{channel} 未配置"}
    
    # 发送测试消息
    title = "🔔 推送测试"
    message = f"这是来自 {channel} 的测试消息，如果收到说明配置正确！"
    
    try:
        # 临时创建只包含该渠道的设置
        test_settings = dict(settings)
        for key in channel_checks:
            if key != channel:
                if key == "wecom":
                    test_settings["wecom_bot_url"] = ""
                elif key == "wxpusher":
                    test_settings["wxpusher_app_token"] = ""
                elif key == "telegram":
                    test_settings["telegram_bot_token"] = ""
                elif key == "bark":
                    test_settings["bark_url"] = ""
                elif key == "gotify":
                    test_settings["gotify_url"] = ""
                elif key == "pushplus":
                    test_settings["pushplus_token"] = ""
                elif key == "serverchan":
                    test_settings["serverchan_key"] = ""
        
        results = send_push_sync(test_settings, title, message, "info")
        
        if results.get(channel):
            return {"ok": True, "message": f"{channel} 推送测试成功！请检查是否收到消息"}
        else:
            return {"ok": False, "message": f"{channel} 推送失败，请检查配置"}
    except Exception as exc:
        return {"ok": False, "message": f"{channel} 推送异常：{str(exc)}"}


def test_all_push_channels(settings: dict[str, Any]) -> dict[str, Any]:
    """测试所有已配置的推送渠道"""
    from .push_service_async import AsyncPushService
    push_service = AsyncPushService(settings)
    enabled = push_service.enabled_channels
    
    if not enabled:
        return {"ok": False, "message": "未配置任何推送渠道", "results": {}}
    
    results = {}
    for channel in enabled:
        result = test_push_channel(settings, channel)
        results[channel] = result
    
    success_count = sum(1 for r in results.values() if r["ok"])
    total = len(results)
    
    return {
        "ok": success_count > 0,
        "message": f"测试完成：{success_count}/{total} 个渠道成功",
        "results": results,
    }
