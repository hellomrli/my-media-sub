from __future__ import annotations

import itertools
from typing import Any

import requests


_counter = itertools.count(1)


class Aria2Client:
    def __init__(self, rpc_url: str, secret: str = ""):
        self.rpc_url = rpc_url.rstrip("/")
        self.secret = secret or ""

    def call(self, method: str, params: list[Any] | None = None, timeout: int = 20):
        if not self.rpc_url:
            raise RuntimeError("Aria2 RPC URL 未配置")
        final_params = []
        if self.secret:
            final_params.append(f"token:{self.secret}")
        if params:
            final_params.extend(params)
        payload = {
            "jsonrpc": "2.0",
            "id": next(_counter),
            "method": method,
            "params": final_params,
        }
        resp = requests.post(self.rpc_url, json=payload, timeout=timeout)
        resp.raise_for_status()
        data = resp.json()
        if "error" in data:
            raise RuntimeError(data["error"].get("message") or str(data["error"]))
        return data.get("result")

    def add_uri(self, urls: list[str], download_dir: str = "", options: dict | None = None):
        """Add a download task.
        
        Args:
            urls: List of URLs to download
            download_dir: Optional download directory
            options: Optional Aria2 options dict (e.g. {"split": "1"})
        """
        opts = options or {}
        if download_dir:
            opts["dir"] = download_dir
        params: list[Any] = [urls]
        if opts:
            params.append(opts)
        return self.call("aria2.addUri", params)

    def get_version(self):
        return self.call("aria2.getVersion", [])
