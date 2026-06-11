from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any
from urllib.parse import urlparse

import httpx
from tenacity import retry, stop_after_attempt, wait_exponential


QUARK_API_BASE = "https://drive.quark.cn/1/clouddrive"


def _set_cookie_value(cookie: str, name: str, value: str) -> str:
    parts = [part.strip() for part in cookie.split(";") if part.strip()]
    replaced = False
    for idx, part in enumerate(parts):
        if part.startswith(f"{name}="):
            parts[idx] = f"{name}={value}"
            replaced = True
            break
    if not replaced:
        parts.append(f"{name}={value}")
    return "; ".join(parts)


@dataclass
class QuarkShareInfo:
    ok: bool
    state: str
    message: str
    files: list[dict[str, Any]]
    file_count: int = 0
    episode_count: int = 0


class QuarkShareProbeAsync:
    """Async version of QuarkShareProbe with retry mechanism."""

    def __init__(self, cookie: str = ""):
        self.cookie = cookie or ""
        self.headers = {
            "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124 Safari/537.36",
            "Accept": "application/json, text/plain, */*",
            "Referer": "https://pan.quark.cn/",
            "Origin": "https://pan.quark.cn",
        }
        if self.cookie:
            self.headers["Cookie"] = self.cookie

    def _refresh_cookie_header(self, resp: httpx.Response) -> None:
        for name in ("__puus", "__pus"):
            value = resp.cookies.get(name)
            if value:
                self.cookie = _set_cookie_value(self.cookie, name, value)
        if self.cookie:
            self.headers["Cookie"] = self.cookie

    @staticmethod
    def extract_pwd_id(url: str) -> str | None:
        parsed = urlparse(url)
        m = re.search(r"/s/([A-Za-z0-9_-]+)", parsed.path)
        return m.group(1) if m else None

    @staticmethod
    def count_episodes(files: list[dict[str, Any]]) -> int:
        names = [str(f.get("name") or "") for f in files]
        patterns = [
            r"(?:^|[^A-Za-z])S\d{1,2}E\d{1,3}(?:[^A-Za-z]|$)",
            r"(?:第\s*\d{1,3}\s*[集话])",
            r"(?:^|[^\d])E\d{1,3}(?:[^\d]|$)",
            r"(?:^|[^\d])\d{1,3}\s*\.\s*(?:mkv|mp4|avi|ts|mov|wmv)$",
        ]
        count = 0
        for name in names:
            lower = name.lower()
            if any(lower.endswith(ext) for ext in [".mkv", ".mp4", ".avi", ".ts", ".mov", ".wmv", ".flv", ".m4v"]):
                if any(re.search(p, name, re.I) for p in patterns):
                    count += 1
        return count

    @retry(stop=stop_after_attempt(3), wait=wait_exponential(multiplier=1, min=2, max=10))
    async def _post(self, client: httpx.AsyncClient, path: str, payload: dict[str, Any], timeout: int = 20) -> dict[str, Any]:
        url = QUARK_API_BASE + path
        params = {"pr": "ucpro", "fr": "pc"}
        resp = await client.post(url, params=params, json=payload, headers=self.headers, timeout=timeout)
        resp.raise_for_status()
        self._refresh_cookie_header(resp)
        return resp.json()

    async def get_share_token(self, client: httpx.AsyncClient, pwd_id: str, passcode: str = "") -> tuple[str | None, str | None]:
        payload = {"pwd_id": pwd_id, "passcode": passcode or ""}
        try:
            data = await self._post(client, "/share/sharepage/token", payload)
        except Exception as e:
            return None, str(e)
        if data.get("code", 0) != 0:
            return None, data.get("message") or data.get("msg") or str(data)
        return data.get("data", ).get("stoken"), None

    async def list_share_files(self, client: httpx.AsyncClient, pwd_id: str, stoken: str, pdir_fid: str = "0") -> tuple[list[dict[str, Any]] | None, str | None]:
        payload = {"pwd_id": pwd_id, "stoken": stoken, "pdir_fid": pdir_fid, "force": 0, "_page": 1, "_size": 200, "_sort": ""}
        try:
            data = await self._post(client, "/share/sharepage/detail", payload)
        except Exception as e:
            return None, str(e)
        if data.get("code", 0) != 0:
            return None, data.get("message") or data.get("msg") or str(data)
        return data.get("data", {}).get("list", []), None

    async def probe(self, url: str, password: str = "") -> QuarkShareInfo:
        pwd_id = self.extract_pwd_id(url)
        if not pwd_id:
            return QuarkShareInfo(False, "invalid_url", "无法从 URL 提取 pwd_id", [])
        
        async with httpx.AsyncClient() as client:
            try:
                stoken, err = await self.get_share_token(client, pwd_id, password)
                if err or not stoken:
                    return QuarkShareInfo(False, "token_failed", err or "获取 token 失败", [])
                
                files, err = await self.list_share_files(client, pwd_id, stoken)
                if err or files is None:
                    return QuarkShareInfo(False, "list_failed", err or "列出文件失败", [])
                
                episode_count = self.count_episodes(files)
                return QuarkShareInfo(True, "ok", "探测成功", files, len(files), episode_count)
            except Exception as e:
                return QuarkShareInfo(False, "error", str(e), [])
