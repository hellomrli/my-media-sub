from __future__ import annotations

import json
import os
import time
from pathlib import Path
from typing import Any
from uuid import uuid4

SUBSCRIPTIONS_PATH = Path(os.getenv("SUBSCRIPTIONS_PATH", "/data/subscriptions.json"))


class SubscriptionStore:
    def __init__(self, path: Path = SUBSCRIPTIONS_PATH):
        self.path = path
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._items: list[dict[str, Any]] = []
        self.load()

    def load(self):
        if self.path.exists():
            try:
                data = json.loads(self.path.read_text())
                self._items = data if isinstance(data, list) else []
            except Exception:
                self._items = []
        else:
            self.save()

    def save(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(".tmp")
        tmp.write_text(json.dumps(self._items, ensure_ascii=False, indent=2))
        tmp.replace(self.path)

    def list(self) -> list[dict[str, Any]]:
        return list(self._items)

    def get(self, sub_id: str) -> dict[str, Any] | None:
        return next((x for x in self._items if x.get("id") == sub_id), None)

    def create_from_item(self, keyword: str, item: dict[str, Any], notify_only: bool = True, media_type: str = "series") -> dict[str, Any]:
        probe = item.get("probe") or {}
        was_invalid = sub.get("status") == "invalid"
        is_invalid = probe.get("state") in {"bad", "invalid_url", "locked"} or (probe.get("ok") is False and probe.get("state") in {"bad", "invalid_url"})
        files = probe.get("files") or []
        known_names = sorted({f.get("name") for f in files if f.get("name")})
        now = int(time.time())
        sub = {
            "id": uuid4().hex[:12],
            "title": keyword or item.get("title") or "未命名订阅",
            "source_title": item.get("title") or "",
            "media_type": media_type,
            "cloud_type": item.get("cloud_type") or "quark",
            "url": item.get("url"),
            "password": item.get("password") or "",
            "known_files": known_names,
            "last_probe": probe,
            "notify_only": notify_only,
            "enabled": True,
            "created_at": now,
            "updated_at": now,
            "last_checked_at": now,
            "last_new_files": [],
            "status": "active",
            "invalid_since": None,
            "last_error": "",
        }
        self._items.append(sub)
        self.save()
        return sub

    def update_check(self, sub_id: str, probe: dict[str, Any]) -> tuple[dict[str, Any] | None, list[str], bool]:
        sub = self.get(sub_id)
        if not sub:
            return None, [], False
        was_invalid = sub.get("status") == "invalid"
        is_invalid = probe.get("state") in {"bad", "invalid_url", "locked"} or (probe.get("ok") is False and probe.get("state") in {"bad", "invalid_url"})
        files = probe.get("files") or []
        names = sorted({f.get("name") for f in files if f.get("name")})
        known = set(sub.get("known_files") or [])
        new_files = [name for name in names if name not in known]
        sub["known_files"] = sorted(known | set(names))
        sub["last_probe"] = probe
        sub["last_new_files"] = new_files
        sub["last_checked_at"] = int(time.time())
        sub["updated_at"] = int(time.time())
        sub["status"] = "invalid" if is_invalid else "active"
        sub["invalid_since"] = sub.get("invalid_since") or int(time.time()) if is_invalid else None
        sub["last_error"] = probe.get("message") or "" if is_invalid else ""
        self.save()
        became_invalid = is_invalid and not was_invalid
        return sub, new_files, became_invalid

    def delete(self, sub_id: str) -> bool:
        before = len(self._items)
        self._items = [x for x in self._items if x.get("id") != sub_id]
        changed = len(self._items) != before
        if changed:
            self.save()
        return changed


subscription_store = SubscriptionStore()
