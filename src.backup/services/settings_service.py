from __future__ import annotations

from pathlib import Path

import requests

from ..clients.quark_save import QuarkSaveClient
from ..stores.settings_store import settings_store
from ..utils.cloud_names import CLOUD_TYPE_NAMES


def get_settings() -> dict:
    data = settings_store.public()
    from .scheduler_service import scheduler_state
    data["subscription_scheduler"] = scheduler_state()
    return data


def update_settings(patch: dict) -> dict:
    settings_store.update(patch)
    return settings_store.public()


def get_cloud_types() -> dict:
    return {"cloud_types": CLOUD_TYPE_NAMES}


def test_quark_cookie(patch: dict | None = None) -> dict:
    settings = settings_store.get()
    patch = patch or {}
    cookie = patch.get("quark_cookie") or settings.get("quark_cookie") or ""
    if not cookie:
        return {"ok": False, "message": "未配置夸克 Cookie"}
    try:
        client = QuarkSaveClient(cookie=cookie)
        items = client.list_dir("0")
    except requests.HTTPError as exc:
        text = exc.response.text[:200] if exc.response is not None else str(exc)
        return {"ok": False, "message": f"夸克请求失败：{text}"}
    except Exception as exc:
        return {"ok": False, "message": f"夸克 Cookie 测试失败：{exc}"}
    if client.cookie != cookie:
        settings_store.update_secret("quark_cookie", client.cookie)
    return {"ok": True, "message": f"夸克 Cookie 可用，根目录可访问，返回 {len(items)} 个条目"}


def test_mount_paths(patch: dict | None = None) -> dict:
    settings = settings_store.get()
    patch = patch or {}
    source = (patch.get("nas_sync_source") or settings.get("nas_sync_source") or "").strip()
    target = (patch.get("nas_sync_target") or settings.get("nas_sync_target") or "").strip()
    if not source or not target:
        return {"ok": False, "message": "未配置源挂载路径或 NAS 目标路径"}

    checked = []
    for label, value, must_write in (("源挂载路径", source, False), ("NAS 目标路径", target, True)):
        path = Path(value).expanduser()
        if not path.exists() or not path.is_dir():
            return {"ok": False, "message": f"{label}不存在或不是目录：{path}"}
        if must_write:
            probe = path / ".my-media-sub-write-test"
            try:
                probe.write_text("ok", encoding="utf-8")
                probe.unlink(missing_ok=True)
            except Exception as exc:
                return {"ok": False, "message": f"{label}不可写：{exc}"}
        checked.append(str(path))
    return {"ok": True, "message": f"挂载路径可访问：{checked[0]} → {checked[1]}"}


def test_nas_sync(patch: dict | None = None) -> dict:
    return test_mount_paths(patch)


def health_payload() -> dict:
    settings = settings_store.get()
    return {
        "status": "ok",
        "search_backend": "inline",
        "mount_backend": "local",
        "auth_enabled": bool(settings.get("app_username") and settings.get("app_password")),
        "check_links": settings.get("check_links"),
        "probe_quark_files": settings.get("probe_quark_files"),
        "filter_bad_links": settings.get("filter_bad_links"),
        "app_name": "Lain 的媒体订阅",
    }
