from __future__ import annotations

import logging
from pathlib import Path
import shutil
from typing import Any

from ..stores.notification_store import notification_store
from ..stores.settings_store import settings_store

logger = logging.getLogger(__name__)


def _join_local_path(prefix: str, relative: str = "") -> Path:
    root = Path(str(prefix or "")).expanduser()
    if not relative:
        return root
    parts = [part for part in str(relative or "").split("/") if part and part not in {".", ".."}]
    return root.joinpath(*parts)


def _result(status: str, message: str, **extra: Any) -> dict[str, Any]:
    return {"status": status, "message": message, **extra}


def sync_to_nas(
    updated: dict[str, Any] | None,
    saved_items: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    """After successful Quark save, optionally copy files via local mount paths.

    This replaces the previous external file-copy API dependency. Users point
    nas_sync_source at the local path where the Quark drive is mounted and
    nas_sync_target at the local/NAS library path.
    """
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

    src_prefix = (settings.get("nas_sync_source") or "").strip()
    dst_prefix = (settings.get("nas_sync_target") or "").strip()
    if not src_prefix or not dst_prefix:
        message = "NAS 同步源挂载路径或目标路径未配置"
        notification_store.add("warning", "nas_sync_not_configured", f"NAS 同步跳过：{title}", message, {"subscription_id": sub_id})
        return [_result("not_configured", message, title=title)]

    names = [item.get("name") or "" for item in saved_items if item.get("name")]
    if not names:
        return [_result("skipped_no_names", "转存结果缺少文件名，无法构造同步任务", title=title)]

    src_dir = _join_local_path(src_prefix, title)
    dst_dir = _join_local_path(dst_prefix, title)
    if not src_dir.exists() or not src_dir.is_dir():
        message = f"源挂载目录不存在或不可读：{src_dir}"
        notification_store.add("warning", "nas_sync_failed", f"NAS 同步失败（源目录）：{title}", message, {"subscription_id": sub_id})
        return [_result("failed", message, title=title, src_dir=str(src_dir), dst_dir=str(dst_dir), names=names)]

    results: list[dict[str, Any]] = []
    try:
        dst_dir.mkdir(parents=True, exist_ok=True)
        for name in names:
            source = src_dir / name
            target = dst_dir / name
            if not source.exists():
                results.append(_result("missing_source", "源文件尚未出现在挂载目录", name=name, title=title, src=str(source), dst=str(target)))
                continue
            if target.exists():
                results.append(_result("skipped_existing", "目标已存在，跳过", name=name, title=title, src=str(source), dst=str(target)))
                continue
            if source.is_dir():
                shutil.copytree(source, target)
            else:
                shutil.copy2(source, target)
            results.append(_result("success", "已复制到 NAS", name=name, title=title, src=str(source), dst=str(target)))
    except Exception as exc:
        message = f"本地挂载复制失败：{exc}"
        logger.warning("NAS sync local copy failed: %s", exc)
        notification_store.add("warning", "nas_sync_failed", f"NAS 同步失败（复制）：{title}", message, {"subscription_id": sub_id})
        return [_result("failed", message, title=title, src_dir=str(src_dir), dst_dir=str(dst_dir), names=names)]

    successes = [item for item in results if item.get("status") == "success"]
    if successes:
        notification_store.add(
            "info",
            "nas_sync_success",
            f"NAS 同步成功：{title}",
            f"已复制 {len(successes)} 个文件到 NAS：{dst_dir}",
            {"subscription_id": sub_id, "files": successes},
        )
    return results
