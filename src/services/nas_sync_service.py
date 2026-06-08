from __future__ import annotations

import logging
from typing import Any

from ..clients.openlist import OpenListClient
from ..stores.notification_store import notification_store
from ..stores.settings_store import settings_store

logger = logging.getLogger(__name__)


def _join_openlist_path(prefix: str, relative: str = "") -> str:
    parts = []
    for value in (prefix, relative):
        parts.extend(part for part in str(value or "").split("/") if part)
    return "/" + "/".join(parts) if parts else "/"


def _result(status: str, message: str, **extra: Any) -> dict[str, Any]:
    return {"status": status, "message": message, **extra}


def sync_to_nas(
    updated: dict[str, Any] | None,
    saved_items: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    """After successful Quark save, optionally copy files to NAS via OpenList."""
    if not updated:
        return [_result("skipped_no_subscription", "没有可同步的订阅信息")]
    title = updated.get("title") or "未命名订阅"
    sub_id = updated.get("id")
    if updated.get("notify_only"):
        return [_result("notify_only", "订阅为仅通知模式，跳过 NAS 同步", title=title)]
    if not saved_items:
        return [_result("skipped_no_quark_success", "没有成功转存的夸克文件，跳过 NAS 同步", title=title)]

    settings = settings_store.get()
    if not settings.get("nas_sync_enabled"):
        return [_result("disabled", "NAS 同步未启用", title=title)]

    openlist_url = (settings.get("openlist_base_url") or "").strip()
    ol_user = (settings.get("openlist_username") or "").strip()
    ol_pass = (settings.get("openlist_password") or "").strip()
    src_prefix = (settings.get("nas_sync_source") or "").strip()
    dst_prefix = (settings.get("nas_sync_target") or "").strip()

    if not openlist_url or not ol_user or not ol_pass:
        message = "OpenList 地址、账号或密码未配置完整"
        notification_store.add("warning", "nas_sync_not_configured", f"NAS 同步跳过：{title}", message, {"subscription_id": sub_id})
        return [_result("not_configured", message, title=title)]
    if not src_prefix or not dst_prefix:
        message = "NAS 同步源路径或目标路径未配置"
        notification_store.add("warning", "nas_sync_not_configured", f"NAS 同步跳过：{title}", message, {"subscription_id": sub_id})
        return [_result("not_configured", message, title=title)]

    names = [item.get("name") or "" for item in saved_items if item.get("name")]
    if not names:
        return [_result("skipped_no_names", "转存结果缺少文件名，无法构造 OpenList 复制请求", title=title)]

    src_dir = _join_openlist_path(src_prefix, title)
    dst_dir = _join_openlist_path(dst_prefix, title)
    client = OpenListClient(openlist_url)
    try:
        client.login(ol_user, ol_pass)
    except Exception as exc:
        message = f"OpenList 登录失败：{exc}"
        logger.warning("OpenList login failed for NAS sync: %s", exc)
        notification_store.add("warning", "nas_sync_failed", f"NAS 同步失败（登录）：{title}", message, {"subscription_id": sub_id})
        return [_result("failed", message, title=title, src_dir=src_dir, dst_dir=dst_dir)]

    try:
        result = client.fs_copy(
            src_dir=src_dir,
            dst_dir=dst_dir,
            names=names,
            overwrite=False,
            skip_existing=True,
            merge=True,
        )
    except Exception as exc:
        message = f"OpenList 复制失败：{exc}"
        logger.warning("NAS sync copy failed: %s", exc)
        notification_store.add("warning", "nas_sync_failed", f"NAS 同步失败（复制）：{title}", message, {"subscription_id": sub_id})
        return [_result("failed", message, title=title, src_dir=src_dir, dst_dir=dst_dir, names=names)]

    code = result.get("code", 0) if isinstance(result, dict) else 0
    if code != 0:
        message = str(result.get("message") or result.get("msg") or result)
        notification_store.add("warning", "nas_sync_failed", f"NAS 同步失败（复制）：{title}", message, {"subscription_id": sub_id})
        return [_result("failed", message, title=title, src_dir=src_dir, dst_dir=dst_dir, names=names)]

    sync_results = [
        _result("success", "已提交 OpenList 复制", name=name, title=title, src_dir=src_dir, dst_dir=dst_dir)
        for name in names
    ]
    notification_store.add(
        "info",
        "nas_sync_success",
        f"NAS 同步成功：{title}",
        f"已复制 {len(sync_results)} 个文件到 NAS：{dst_dir}",
        {"subscription_id": sub_id, "files": sync_results},
    )
    return sync_results
