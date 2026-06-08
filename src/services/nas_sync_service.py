from __future__ import annotations

import logging
from typing import Any

from ..clients.openlist import OpenListClient
from ..stores.notification_store import notification_store
from ..stores.settings_store import settings_store

logger = logging.getLogger(__name__)


def sync_to_nas(
    updated: dict[str, Any] | None,
    saved_items: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    """After successful Quark save, optionally copy files to NAS via OpenList.

    Conditions:
    - Subscription exists and notify_only is False.
    - There are successfully saved Quark items.
    - Global 'nas_sync_enabled' setting is True.
    - OpenList base URL, username, password are configured.
    - nas_sync_source and nas_sync_target are configured.

    Returns a list of sync results (empty list if nothing was synced).
    """
    if not updated or updated.get("notify_only"):
        return []
    if not saved_items:
        return []
    settings = settings_store.get()
    if not settings.get("nas_sync_enabled"):
        return []

    openlist_url = (settings.get("openlist_base_url") or "").strip()
    ol_user = (settings.get("openlist_username") or "").strip()
    ol_pass = (settings.get("openlist_password") or "").strip()
    src_prefix = (settings.get("nas_sync_source") or "").strip()
    dst_prefix = (settings.get("nas_sync_target") or "").strip()

    if not openlist_url or not ol_user or not ol_pass:
        return []
    if not src_prefix or not dst_prefix:
        return []

    names = [item.get("name") or "" for item in saved_items if item.get("name")]
    if not names:
        return []

    title = updated.get("title") or "未命名订阅"
    sub_id = updated.get("id")

    # Build source and target directories
    src_dir = f"{src_prefix.rstrip('/')}/{title}"
    dst_dir = f"{dst_prefix.rstrip('/')}/{title}"
    # Login to OpenList
    client = OpenListClient(openlist_url)
    try:
        client.login(ol_user, ol_pass)
    except Exception as exc:
        logger.warning("OpenList login failed for NAS sync: %s", exc)
        notification_store.add(
            "warning",
            "nas_sync_failed",
            f"NAS 同步失败（登录）：{title}",
            str(exc),
            {"subscription_id": sub_id},
        )
        return []

    # Execute copy
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
        logger.warning("NAS sync copy failed: %s", exc)
        notification_store.add(
            "warning",
            "nas_sync_failed",
            f"NAS 同步失败（复制）：{title}",
            str(exc),
            {"subscription_id": sub_id},
        )
        return []

    # Build sync result list
    sync_results = []
    for name in names:
        sync_results.append({"name": name, "src_dir": src_dir, "dst_dir": dst_dir})

    notification_store.add(
        "info",
        "nas_sync_success",
        f"NAS 同步成功：{title}",
        f"已复制 {len(sync_results)} 个文件到 NAS：{dst_dir}",
        {"subscription_id": sub_id, "files": sync_results},
    )
    return sync_results
