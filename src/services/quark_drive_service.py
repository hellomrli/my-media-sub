from __future__ import annotations

from typing import Any

import requests

from ..clients.aria2 import Aria2Client
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


def download_from_quark(fid: str, file_name: str | None = None, download_dir: str | None = None, use_proxy: bool = True) -> dict[str, Any]:
    """Get a download URL from Quark drive and submit it to Aria2.
    
    Args:
        fid: Quark file ID
        file_name: Optional custom filename
        download_dir: Optional download directory
        use_proxy: If True, use local proxy endpoint to bypass UA/Referer checks (default: True)
    """
    settings = settings_store.get()
    old_cookie = settings.get("quark_cookie") or ""
    
    # Get file metadata
    try:
        client = _client()
        data = client.get_download_urls([fid])
    except Exception as exc:
        return {"ok": False, "message": f"夸克获取直链失败：{exc}"}
    _persist_cookie(client, old_cookie)

    err = _quark_error(data)
    if err:
        return {"ok": False, "message": f"夸克接口返回错误：{err}"}

    items = (data.get("data") or [])
    if not items:
        return {"ok": False, "message": "夸克未返回下载链接"}

    item = items[0]
    direct_url = item.get("url") or item.get("download_url") or ""
    name = file_name or item.get("file_name") or fid

    if not direct_url:
        return {"ok": False, "message": f"文件 {name} 无可用下载链接"}

    # Submit to Aria2
    aria2_url = settings.get("aria2_rpc_url") or ""
    aria2_secret = settings.get("aria2_secret") or ""
    final_dir = download_dir or settings.get("aria2_dir") or ""

    if not aria2_url:
        return {"ok": False, "message": "Aria2 RPC URL 未配置"}

    # Use proxy URL to bypass CDN restrictions
    if use_proxy:
        # Store the direct URL temporarily (in production, use Redis or DB)
        import os
        proxy_url = f"http://{os.getenv('PROXY_HOST', '192.168.50.160')}:{os.getenv('BOT_PORT', '8788')}/api/quark-proxy/download?fid={fid}"
        download_url = proxy_url
    else:
        download_url = direct_url

    try:
        aria2 = Aria2Client(aria2_url, aria2_secret)
        # Disable split download for proxy URLs (our proxy doesn't support Range requests efficiently)
        options = {"split": "1", "max-connection-per-server": "1"} if use_proxy else {}
        gid = aria2.add_uri([download_url], final_dir, options=options)
    except Exception as exc:
        return {"ok": False, "message": f"Aria2 提交失败：{exc}"}

    return {
        "ok": True,
        "gid": gid,
        "fid": fid,
        "file_name": name,
        "url": download_url,
        "direct_url": direct_url if use_proxy else None,
        "dir": final_dir,
    }
