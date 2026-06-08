from __future__ import annotations

from typing import Any

import requests

from ..clients.openlist import OpenListClient
from ..clients.quark_save import QuarkSaveClient
from ..stores.settings_store import settings_store
from ..utils.cloud_names import CLOUD_TYPE_NAMES


def _first_error_message(data: dict[str, Any]) -> str:
    return str(data.get("message") or data.get("msg") or data.get("error") or data)


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


def test_openlist(patch: dict | None = None) -> dict:
    settings = settings_store.get()
    patch = patch or {}
    base_url = (patch.get("openlist_base_url") or settings.get("openlist_base_url") or "").strip().rstrip("/")
    username = (patch.get("openlist_username") or settings.get("openlist_username") or "").strip()
    password = patch.get("openlist_password") or settings.get("openlist_password") or ""
    if not base_url:
        return {"ok": False, "message": "未配置 OpenList 地址"}
    if not username or not password:
        return {"ok": False, "message": "未配置 OpenList 账号或密码"}
    try:
        client = OpenListClient(base_url)
        client.login(username, password)
    except Exception as exc:
        return {"ok": False, "message": f"OpenList 登录失败：{exc}"}
    return {"ok": True, "message": "OpenList 登录成功"}


def test_nas_sync(patch: dict | None = None) -> dict:
    settings = settings_store.get()
    patch = patch or {}
    openlist_result = test_openlist(patch)
    if not openlist_result.get("ok"):
        return openlist_result

    base_url = (patch.get("openlist_base_url") or settings.get("openlist_base_url") or "").strip().rstrip("/")
    username = (patch.get("openlist_username") or settings.get("openlist_username") or "").strip()
    password = patch.get("openlist_password") or settings.get("openlist_password") or ""
    source = (patch.get("nas_sync_source") or settings.get("nas_sync_source") or "").strip()
    target = (patch.get("nas_sync_target") or settings.get("nas_sync_target") or "").strip()
    if not source or not target:
        return {"ok": False, "message": "未配置 NAS 同步源路径或目标路径"}

    client = OpenListClient(base_url)
    try:
        client.login(username, password)
        checked = []
        for label, path in (("源路径", source), ("目标路径", target)):
            data = client.fs_list(path)
            if data.get("code", 0) != 0:
                return {"ok": False, "message": f"{label}不可访问：{_first_error_message(data)}"}
            checked.append(path)
    except Exception as exc:
        return {"ok": False, "message": f"NAS 路径测试失败：{exc}"}
    return {"ok": True, "message": f"NAS 同步路径可访问：{checked[0]} → {checked[1]}"}


def health_payload() -> dict:
    settings = settings_store.get()
    return {
        "status": "ok",
        "pansou_base_url": settings.get("pansou_base_url"),
        "openlist_base_url": settings.get("openlist_base_url"),
        "auth_enabled": bool(settings.get("app_username") and settings.get("app_password")),
        "check_links": settings.get("check_links"),
        "probe_quark_files": settings.get("probe_quark_files"),
        "filter_bad_links": settings.get("filter_bad_links"),
        "app_name": "Lain 的媒体订阅",
    }
