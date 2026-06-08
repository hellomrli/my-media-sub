from __future__ import annotations

from typing import Any

from ..clients.aria2 import Aria2Client
from ..stores.settings_store import settings_store
from .search_service import sessions


def current_settings() -> dict[str, Any]:
    return settings_store.get()


def test_aria2() -> dict[str, Any]:
    settings = current_settings()
    client = Aria2Client(settings.get("aria2_rpc_url") or "", settings.get("aria2_secret") or "")
    return {"version": client.get_version()}


def download_with_aria2(chat_id: str, index: int | None = None, url: str | None = None, download_dir: str | None = None) -> dict[str, Any]:
    settings = current_settings()
    selected = None
    final_url = url
    if index is not None:
        sess = sessions.get(chat_id)
        if not sess:
            raise LookupError("没有找到最近的搜索结果，请先搜索。")
        if index > len(sess.results):
            raise ValueError("选择编号超出范围。")
        selected = sess.results[index - 1]
        final_url = selected.get("url")
    if not final_url:
        raise ValueError("缺少下载链接")
    client = Aria2Client(settings.get("aria2_rpc_url") or "", settings.get("aria2_secret") or "")
    gid = client.add_uri([final_url], download_dir or settings.get("aria2_dir") or "")
    return {"gid": gid, "url": final_url, "selected": selected}


def download_urls_with_aria2(urls: list[str], download_dir: str | None = None) -> list[dict[str, Any]]:
    settings = current_settings()
    client = Aria2Client(settings.get("aria2_rpc_url") or "", settings.get("aria2_secret") or "")
    final_dir = download_dir or settings.get("aria2_dir") or ""
    results = []
    for url in urls:
        gid = client.add_uri([url], final_dir)
        results.append({"gid": gid, "url": url, "dir": final_dir})
    return results
