from __future__ import annotations

import json
import os
import time
from pathlib import Path
from typing import Any
from uuid import uuid4

NOTIFICATIONS_PATH = Path(os.getenv("NOTIFICATIONS_PATH", "/data/notifications.json"))


class NotificationStore:
    def __init__(self, path: Path = NOTIFICATIONS_PATH):
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
        tmp.write_text(json.dumps(self._items[-300:], ensure_ascii=False, indent=2))
        tmp.replace(self.path)

    def add(self, level: str, event: str, title: str, message: str, meta: dict[str, Any] | None = None) -> dict[str, Any]:
        item = {
            "id": uuid4().hex[:12],
            "level": level,
            "event": event,
            "title": title,
            "message": message,
            "meta": meta or {},
            "read": False,
            "created_at": int(time.time()),
        }
        self._items.append(item)
        self.save()
        return item

    def list(self, include_read: bool = True) -> list[dict[str, Any]]:
        items = list(reversed(self._items))
        if not include_read:
            items = [x for x in items if not x.get("read")]
        return items

    def mark_read(self, notification_id: str | None = None):
        for item in self._items:
            if notification_id is None or item.get("id") == notification_id:
                item["read"] = True
        self.save()


notification_store = NotificationStore()
