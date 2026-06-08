from __future__ import annotations

import logging
from typing import Any

import requests

logger = logging.getLogger(__name__)

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


class QuarkSaveClient:
    """Save files from a Quark share link to the user's own Quark drive."""

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

    def _refresh_cookie_header(self, resp: requests.Response) -> None:
        for name in ("__puus", "__pus"):
            value = resp.cookies.get(name)
            if value:
                self.cookie = _set_cookie_value(self.cookie, name, value)
        if self.cookie:
            self.session.headers.update({"Cookie": self.cookie})

    def _get(self, path: str, params: dict[str, Any] | None = None, timeout: int = 20) -> dict[str, Any]:
        url = QUARK_API_BASE + path
        params = dict(params or {})
        params.setdefault("pr", "ucpro")
        params.setdefault("fr", "pc")
        resp = self.session.get(url, params=params, timeout=timeout)
        resp.raise_for_status()
        self._refresh_cookie_header(resp)
        return resp.json()

    def _post(self, path: str, payload: dict[str, Any] | None = None, timeout: int = 20) -> dict[str, Any]:
        url = QUARK_API_BASE + path
        params = {"pr": "ucpro", "fr": "pc"}
        resp = self.session.post(url, params=params, json=payload or {}, timeout=timeout)
        resp.raise_for_status()
        self._refresh_cookie_header(resp)
        return resp.json()

    # ── Target directory management ──────────────────────────────────

    def list_dir(self, parent_fid: str = "0") -> list[dict[str, Any]]:
        data = self._get("/file/sort", params={"pid": parent_fid, "_page": "1", "_size": "200"})
        return (data.get("data") or {}).get("list") or []

    @staticmethod
    def normalize_item(item: dict[str, Any]) -> dict[str, Any]:
        fid = item.get("fid") or item.get("file_id") or ""
        name = item.get("file_name") or item.get("name") or ""
        is_dir = bool(item.get("dir") or item.get("file") is False or item.get("file_type") == 0)
        return {
            "fid": fid,
            "name": name,
            "is_dir": is_dir,
            "size": item.get("size") or 0,
            "updated_at": item.get("updated_at") or item.get("last_update_at") or item.get("created_at") or "",
            "raw_type": item.get("file_type"),
        }

    def create_dir(self, parent_fid: str, name: str) -> str | None:
        payload = {"pdir_fid": parent_fid, "file_name": [name], "dir_init": True}
        data = self._post("/file", payload)
        result = data.get("data", {})
        if isinstance(result, list) and result:
            return result[0].get("fid")
        if isinstance(result, dict):
            return result.get("fid")
        return None

    def ensure_dir_path(self, path: str) -> str:
        parent_fid = "0"
        for part in [p for p in path.strip("/").split("/") if p]:
            items = self.list_dir(parent_fid)
            found = None
            for item in items:
                name = item.get("file_name") or item.get("name") or ""
                is_dir = bool(item.get("dir") or item.get("file") is False or item.get("file_type") == 0)
                if is_dir and name == part:
                    found = item.get("fid") or item.get("file_id")
                    break
            if found:
                parent_fid = found
            else:
                created = self.create_dir(parent_fid, part)
                if not created:
                    raise RuntimeError(f"无法创建夸克目录 {parent_fid}/{part}")
                parent_fid = created
        return parent_fid

    def delete_items(self, fids: list[str]) -> dict[str, Any]:
        return self._post("/file/delete", {"action_type": 2, "filelist": fids})

    def rename_item(self, fid: str, name: str) -> dict[str, Any]:
        return self._post("/file/rename", {"fid": fid, "file_name": name})

    # ── Save share files ──────────────────────────────────────────────

    def save_share_files(
        self,
        pwd_id: str,
        stoken: str,
        fid_list: list[str],
        fid_token_list: list[str],
        to_pdir_fid: str = "0",
    ) -> dict[str, Any]:
        payload = {
            "fid_list": fid_list,
            "fid_token_list": fid_token_list,
            "to_pdir_fid": to_pdir_fid,
            "pwd_id": pwd_id,
            "stoken": stoken,
        }
        return self._post("/share/sharepage/save", payload)

    def save_entire_share(self, pwd_id: str, stoken: str, top_files: list[dict[str, Any]], to_pdir_fid: str = "0") -> dict[str, Any]:
        fid_list, fid_token_list = [], []
        for f in top_files:
            fid = f.get("fid")
            token = f.get("share_fid_token") or ""
            if fid and token:
                fid_list.append(fid)
                fid_token_list.append(token)
        if not fid_list:
            return {"code": 1, "message": "没有可转存的文件"}
        return self.save_share_files(pwd_id, stoken, fid_list, fid_token_list, to_pdir_fid)
