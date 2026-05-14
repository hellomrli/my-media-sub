from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any
from urllib.parse import urlparse

import requests


@dataclass
class QuarkShareInfo:
    ok: bool
    state: str
    message: str
    files: list[dict[str, Any]]
    file_count: int = 0
    episode_count: int = 0


class QuarkShareProbe:
    """Best-effort Quark public share explorer.

    It uses Quark's public share APIs. These endpoints may change or trigger
    captcha/risk-control; failures are returned as structured states instead of
    crashing the app.
    """

    def __init__(self, cookie: str = ""):
        self.cookie = cookie or ""
        self.session = requests.Session()
        self.session.headers.update({
            "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124 Safari/537.36",
            "Accept": "application/json, text/plain, */*",
            "Referer": "https://pan.quark.cn/",
            "Origin": "https://pan.quark.cn",
        })
        if self.cookie:
            self.session.headers.update({"Cookie": self.cookie})

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

    def _post(self, path: str, payload: dict[str, Any], timeout: int = 20) -> dict[str, Any]:
        url = "https://drive-pc.quark.cn/1/clouddrive" + path
        params = {"pr": "ucpro", "fr": "pc"}
        resp = self.session.post(url, params=params, json=payload, timeout=timeout)
        resp.raise_for_status()
        return resp.json()

    def get_share_token(self, pwd_id: str, passcode: str = "") -> tuple[str | None, str | None]:
        payload = {"pwd_id": pwd_id, "passcode": passcode or ""}
        data = self._post("/share/sharepage/token", payload)
        if data.get("code", 0) != 0:
            return None, data.get("message") or data.get("msg") or str(data)
        token = (data.get("data") or {}).get("stoken")
        return token, None

    def list_files(self, pwd_id: str, stoken: str, pdir_fid: str = "0", page: int = 1, size: int = 100) -> tuple[list[dict], str | None]:
        payload = {
            "pwd_id": pwd_id,
            "stoken": stoken,
            "pdir_fid": pdir_fid,
            "force": 0,
            "_page": page,
            "_size": size,
            "_fetch_total": 1,
            "_fetch_sub_dirs": 0,
            "_sort": "file_type:asc,file_name:asc",
        }
        data = self._post("/share/sharepage/detail", payload)
        if data.get("code", 0) != 0:
            return [], data.get("message") or data.get("msg") or str(data)
        raw_list = (data.get("data") or {}).get("list") or []
        return raw_list, None

    def probe(self, url: str, passcode: str = "", max_files: int = 300) -> QuarkShareInfo:
        pwd_id = self.extract_pwd_id(url)
        if not pwd_id:
            return QuarkShareInfo(False, "invalid_url", "不是有效的夸克分享链接", [])

        try:
            stoken, err = self.get_share_token(pwd_id, passcode)
            if err:
                state = "locked" if "提取码" in err or "密码" in err or "pass" in err.lower() else "bad"
                return QuarkShareInfo(False, state, err, [])
            if not stoken:
                return QuarkShareInfo(False, "bad", "未能获取分享 token", [])

            raw, err = self.list_files(pwd_id, stoken)
            if err:
                return QuarkShareInfo(False, "bad", err, [])

            files: list[dict[str, Any]] = []
            queue = list(raw)
            while queue and len(files) < max_files:
                item = queue.pop(0)
                fid = item.get("fid") or item.get("file_id")
                name = item.get("file_name") or item.get("name") or ""
                is_dir = bool(item.get("dir") or item.get("file_type") == 0 and item.get("obj_category") == "dir")
                normalized = {
                    "name": name,
                    "fid": fid,
                    "is_dir": is_dir,
                    "size": item.get("size") or 0,
                    "category": item.get("category"),
                    "format_type": item.get("format_type") or item.get("file_type"),
                }
                files.append(normalized)
                if is_dir and fid and len(files) < max_files:
                    children, child_err = self.list_files(pwd_id, stoken, pdir_fid=fid)
                    if not child_err:
                        queue.extend(children)

            episode_count = self.count_episodes(files)
            return QuarkShareInfo(True, "ok", "链接可访问", files, len(files), episode_count)
        except requests.HTTPError as e:
            text = e.response.text[:300] if e.response is not None else str(e)
            return QuarkShareInfo(False, "http_error", text, [])
        except Exception as e:
            return QuarkShareInfo(False, "error", str(e), [])
