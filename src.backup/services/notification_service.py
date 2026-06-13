from __future__ import annotations

from ..stores.notification_store import notification_store


def list_notifications(include_read: bool = True) -> dict:
    return {"notifications": notification_store.list(include_read=include_read)}


def mark_notification_read(notification_id: str | None = None) -> dict:
    notification_store.mark_read(notification_id)
    return {"ok": True}
