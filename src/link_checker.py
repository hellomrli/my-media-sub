from __future__ import annotations

import requests


class PanSouLinkChecker:
    def __init__(self, base_url: str):
        self.base_url = base_url.rstrip('/')

    def check_quark(self, items: list[dict], timeout: int = 30) -> list[dict]:
        if not items:
            return []
        payload = {
            "items": [
                {
                    "disk_type": "quark",
                    "url": item.get("url"),
                    "password": item.get("password") or "",
                }
                for item in items
                if item.get("url")
            ]
        }
        if not payload["items"]:
            return []
        resp = requests.post(f"{self.base_url}/api/check/links", json=payload, timeout=timeout)
        resp.raise_for_status()
        data = resp.json()
        if isinstance(data, dict) and "data" in data and isinstance(data["data"], dict):
            return data["data"].get("results", []) or []
        return data.get("results", []) or []
