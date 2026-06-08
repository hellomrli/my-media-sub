from __future__ import annotations

from typing import Any

import requests

from ..clients.quark_save import QuarkSaveClient
from ..stores.settings_store import settings_store


def _client() -> QuarkSaveClient:
    cookie = settings_store.get().get("quark_cookie") or ""
    if not cookie:
        raise RuntimeError("未配置夸克 Cookie")
    return QuarkSaveClient(cookie=cookie)


def _persist_cookie(client: QuarkSaveClient, old_cookie: str | None = None) -> None:
    if client.cookie and client.cookie != old_cookie:
        settings_store.update_secret("quark_cookie", client.cookie)


def _quark_error(data: dict[str, Any]) -> str | None:
    if data.get("code", 0) == 0:
        return None
    return str(data.get("message") or data.get("msg") or data)


def list_drive(parent_fid: str = "0") -> dict[str, Any]:
    settings = settings_store.get()
    old_cookie = settings.get("quark_cookie") or ""
    try:
        client = _client()
        items = client.list_dir(parent_fid or "0")
    except requests.HTTPError as exc:
        text = exc.response.text[:200] if exc.response is not None else str(exc)
        return {"ok": False, "message": f"夸克请求失败：{text}", "items": []}
    except Exception as exc:
        return {"ok": False, "message": str(exc), "items": []}
    _persist_cookie(client, old_cookie)
    normalized = [client.normalize_item(item) for item in items]
    normalized.sort(key=lambda item: (not item["is_dir"], item["name"].lower()))
    return {"ok": True, "parent_fid": parent_fid or "0", "items": normalized}


def create_folder(parent_fid: str, name: str) -> dict[str, Any]:
    name = (name or "").strip()
    if not name:
        return {"ok": False, "message": "文件夹名称不能为空"}
    old_cookie = settings_store.get().get("quark_cookie") or ""
    try:
        client = _client()
        fid = client.create_dir(parent_fid or "0", name)
    except Exception as exc:
        return {"ok": False, "message": f"创建文件夹失败：{exc}"}
    _persist_cookie(client, old_cookie)
    if not fid:
        return {"ok": False, "message": "夸克未返回新文件夹 ID"}
    return {"ok": True, "message": f"已创建文件夹：{name}", "fid": fid}


def rename_item(fid: str, name: str) -> dict[str, Any]:
    fid = (fid or "").strip()
    name = (name or "").strip()
    if not fid or not name:
        return {"ok": False, "message": "缺少文件 ID 或新名称"}
    old_cookie = settings_store.get().get("quark_cookie") or ""
    try:
        client = _client()
        data = client.rename_item(fid, name)
    except Exception as exc:
        return {"ok": False, "message": f"重命名失败：{exc}"}
    _persist_cookie(client, old_cookie)
    err = _quark_error(data)
    if err:
        return {"ok": False, "message": err}
    return {"ok": True, "message": f"已重命名为：{name}"}


def delete_items(fids: list[str]) -> dict[str, Any]:
    fids = [fid for fid in fids if fid]
    if not fids:
        return {"ok": False, "message": "请选择要删除的文件"}
    old_cookie = settings_store.get().get("quark_cookie") or ""
    try:
        client = _client()
        data = client.delete_items(fids)
    except Exception as exc:
        return {"ok": False, "message": f"删除失败：{exc}"}
    _persist_cookie(client, old_cookie)
    err = _quark_error(data)
    if err:
        return {"ok": False, "message": err}
    return {"ok": True, "message": f"已删除 {len(fids)} 项"}
