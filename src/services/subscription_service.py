from __future__ import annotations

from typing import Any

from ..clients.quark import QuarkShareProbe
from ..stores.notification_store import notification_store
from ..stores.settings_store import settings_store
from ..stores.subscription_store import subscription_store
from .download_service import download_urls_with_aria2
from .nas_sync_service import sync_to_nas
from .push_service import PushScenarios
from .push_helper import send_push_background
from .quark_save_service import save_subscription_transfers
from .search_service import sessions
from .transfer_rule_service import build_transfer_plan


def create_subscription_from_selection(chat_id: str, index: int, media_type: str, notify_only: bool) -> dict[str, Any]:
    sess = sessions.get(chat_id)
    if not sess:
        raise LookupError("没有找到最近的搜索结果，请先搜索。")
    if index > len(sess.results):
        raise ValueError("选择编号超出范围。")
    item = sess.results[index - 1]
    if item.get("cloud_type") != "quark":
        raise ValueError("当前订阅 MVP 只支持夸克分享链接嗅探更新。")
    if media_type == "movie":
        raise ValueError("电影通常不会追更，请使用选择或下载，不创建订阅。")
    if media_type not in {"series", "anime"}:
        raise ValueError("媒体类型只能是 movie / series / anime。")
    return subscription_store.create_from_item(sess.keyword, item, notify_only=notify_only, media_type=media_type)


def probe_subscription(sub: dict[str, Any]) -> dict[str, Any]:
    info = QuarkShareProbe().probe(sub.get("url") or "", sub.get("password") or "")
    return {
        "ok": info.ok,
        "state": info.state,
        "message": info.message,
        "files": info.files[:300],
        "file_count": info.file_count,
        "episode_count": info.episode_count,
    }


def update_subscription(subscription_id: str, patch: dict[str, Any]) -> dict[str, Any] | None:
    result = subscription_store.update(subscription_id, patch)
    
    # 如果设置为完结，在后台异步发送推送（不阻塞）
    if result and patch.get("completed"):
        settings = settings_store.get()
        if settings.get("push_on_completed", True):
            title, message, level, scenario = PushScenarios.subscription_completed(result.get("title", "未知订阅"))
            send_push_background(settings, title, message, level, scenario=scenario)
    
    return result


def plan_subscription(subscription_id: str, files: list[dict[str, Any]] | None = None, rules: dict[str, Any] | None = None, target_existing_files: list[str] | None = None, target_dir_exists: bool | None = None) -> dict[str, Any]:
    sub = subscription_store.get(subscription_id)
    if not sub:
        raise LookupError("订阅不存在。")
    if rules is not None:
        sub = dict(sub)
        sub["rules"] = rules
    probe_files = files if files is not None else (sub.get("last_probe") or {}).get("files") or []
    return build_transfer_plan(sub, probe_files, target_existing_files=target_existing_files, target_dir_exists=target_dir_exists)


def check_subscription(subscription_id: str) -> tuple[dict[str, Any] | None, list[str], bool, list[dict[str, Any]], list[dict[str, Any]], list[dict[str, Any]]]:
    sub = subscription_store.get(subscription_id)
    if not sub:
        raise LookupError("订阅不存在。")
    probe = probe_subscription(sub)
    plan = build_transfer_plan(sub, probe.get("files") or [])
    updated, new_files, became_invalid = subscription_store.update_check(subscription_id, probe, plan)
    add_check_notifications(updated, new_files, became_invalid)
    
    # 在后台异步推送通知（不阻塞订阅检查）
    settings = settings_store.get()
    sub_title = updated.get("title") if updated else sub.get("title")
    
    if became_invalid and settings.get("push_on_failed", True):
        title, message, level, scenario = PushScenarios.subscription_failed(sub_title, probe.get("message", "未知错误"))
        send_push_background(settings, title, message, level, scenario=scenario)
    elif new_files and settings.get("push_on_update", True):
        items = [{"title": f} for f in new_files]
        title, message, level, scenario = PushScenarios.subscription_update(sub_title, items)
        send_push_background(settings, title, message, level, silent=settings.get("push_silent", False), scenario=scenario)
    
    downloads = maybe_download_new_items(updated, plan)
    quark_saves = save_subscription_transfers(updated, plan)
    
    if quark_saves and settings.get("push_on_save", True):
        title, message, level, scenario = PushScenarios.save_completed(sub_title, len(quark_saves))
        send_push_background(settings, title, message, level, silent=settings.get("push_silent", False), scenario=scenario)
    
    nas_syncs = sync_to_nas(updated, quark_saves)
    return updated, new_files, became_invalid, downloads, quark_saves, nas_syncs


def check_all_subscriptions() -> list[dict[str, Any]]:
    results = []
    settings = settings_store.get()
    push_service = get_push_service(settings)
    
    for sub in subscription_store.list():
        if not sub.get("enabled", True) or sub.get("completed"):
            continue
        probe = probe_subscription(sub)
        plan = build_transfer_plan(sub, probe.get("files") or [])
        updated, new_files, became_invalid = subscription_store.update_check(sub["id"], probe, plan)
        add_check_notifications(updated, new_files, became_invalid)
        
        # 推送通知
        sub_title = updated.get("title") if updated else sub.get("title")
        if became_invalid and settings.get("push_on_failed", True):
            title, message, level, scenario = PushScenarios.subscription_failed(sub_title, probe.get("message", "未知错误"))
            push_service.send(title, message, level, scenario=scenario)
        elif new_files and settings.get("push_on_update", True):
            items = [{"title": f} for f in new_files]
            title, message, level, scenario = PushScenarios.subscription_update(sub_title, items)
            push_service.send(title, message, level, silent=settings.get("push_silent", False))
        
        downloads = maybe_download_new_items(updated, plan)
        quark_saves = save_subscription_transfers(updated, plan)
        
        if quark_saves and settings.get("push_on_save", True):
            title, message, level, scenario = PushScenarios.save_completed(sub_title, len(quark_saves))
            push_service.send(title, message, level, silent=settings.get("push_silent", False))
        
        nas_syncs = sync_to_nas(updated, quark_saves)
        results.append({"subscription": updated, "new_files": new_files, "became_invalid": became_invalid, "downloads": downloads, "quark_saves": quark_saves, "nas_syncs": nas_syncs})
    return results


def maybe_download_new_items(updated: dict[str, Any] | None, plan: dict[str, Any]) -> list[dict[str, Any]]:
    if not updated or updated.get("notify_only"):
        return []
    settings = settings_store.get()
    if not settings.get("auto_download_new_subscription_items"):
        return []
    transfers = plan.get("transfers") or []
    source_url = updated.get("url") or ""
    urls = [source_url] if transfers and source_url else []
    if not urls:
        return []
    try:
        downloads = download_urls_with_aria2(urls, settings.get("aria2_dir"))
    except Exception as exc:
        notification_store.add(
            "warning",
            "subscription_download_failed",
            f"订阅自动下载失败：{updated.get('title')}",
            str(exc),
            {"subscription_id": updated.get("id"), "url": updated.get("url")},
        )
        return []
    subscription_store.mark_transferred(updated["id"], transfers)
    notification_store.add(
        "info",
        "subscription_download_started",
        f"订阅已发送到 Aria2：{updated.get('title')}",
        f"已提交 {len(downloads)} 个下载任务。",
        {"subscription_id": updated.get("id"), "downloads": downloads},
    )
    return downloads


def add_check_notifications(updated: dict[str, Any] | None, new_files: list[str], became_invalid: bool) -> None:
    if updated and became_invalid:
        notification_store.add(
            "warning",
            "subscription_invalid",
            f"订阅链接疑似失效：{updated.get('title')}",
            updated.get("last_error") or "链接检查失败或分享不可访问",
            {"subscription_id": updated.get("id"), "url": updated.get("url")},
        )
    if updated and new_files:
        notification_store.add(
            "info",
            "subscription_updated",
            f"订阅有更新：{updated.get('title')}",
            "发现新文件：" + "、".join(new_files[:10]),
            {"subscription_id": updated.get("id"), "new_files": new_files},
        )
