from __future__ import annotations

import time

from fastapi import APIRouter, Query, Response
from fastapi.responses import StreamingResponse

from ..clients.quark_save import QuarkSaveClient
from ..stores.settings_store import settings_store

router = APIRouter()

# Simple in-memory cache for direct URLs (fid -> (url, expire_time))
_url_cache: dict[str, tuple[str, float]] = {}


def get_cached_url(fid: str, cookie: str) -> str:
    """Get direct URL from cache or fetch from Quark API."""
    now = time.time()

    # Check cache
    if fid in _url_cache:
        url, expire = _url_cache[fid]
        if now < expire:
            return url

    # Fetch fresh URL
    client = QuarkSaveClient(cookie=cookie)
    data = client.get_download_urls([fid])
    items = data.get("data", [])
    if not items:
        raise ValueError("夸克未返回下载链接")

    direct_url = items[0].get("url") or items[0].get("download_url") or ""
    if not direct_url:
        raise ValueError("无可用下载链接")

    # Cache for 1 hour
    _url_cache[fid] = (direct_url, now + 3600)
    return direct_url


@router.get("/api/quark-proxy/download")
def proxy_download(fid: str = Query(...)):
    """Proxy download from Quark to bypass CDN UA/Referer restrictions.

    Aria2 requests this endpoint, which then fetches from Quark with proper headers.
    Caches direct URLs to support Aria2's multi-connection downloads.
    """
    settings = settings_store.get()
    cookie = settings.get("quark_cookie") or ""

    if not cookie:
        return Response(content="未配置夸克 Cookie", status_code=500)

    # Get direct URL (cached or fresh)
    try:
        direct_url = get_cached_url(fid, cookie)
    except Exception as exc:
        return Response(content=f"获取下载链接失败：{exc}", status_code=500)

    # Fetch from Quark with proper headers and stream back
    import requests

    headers = {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124 Safari/537.36",
        "Referer": "https://pan.quark.cn/",
        "Origin": "https://pan.quark.cn",
        "Accept": "*/*",
        "Accept-Encoding": "identity",  # Disable compression for streaming
        "Connection": "keep-alive",
        "Cookie": cookie,
    }

    try:
        resp = requests.get(direct_url, headers=headers, stream=True, timeout=(10, 300))  # (connect, read)
        resp.raise_for_status()
    except Exception as exc:
        return Response(content=f"从夸克下载失败：{exc}", status_code=502)

    # Stream response back to Aria2
    def generate():
        try:
            for chunk in resp.iter_content(chunk_size=65536):  # Larger chunks for better throughput
                if chunk:
                    yield chunk
        finally:
            resp.close()

    return StreamingResponse(
        generate(),
        media_type=resp.headers.get("Content-Type", "application/octet-stream"),
        headers={
            "Content-Length": resp.headers.get("Content-Length", ""),
            "Content-Disposition": resp.headers.get("Content-Disposition", ""),
            "Accept-Ranges": "bytes",
        },
    )
