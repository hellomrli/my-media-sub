from __future__ import annotations

from fastapi import APIRouter, Depends

from ..auth import require_auth
from ..schemas.requests import MarkNotificationReadRequest
from ..services.notification_service import list_notifications, mark_notification_read

router = APIRouter(dependencies=[Depends(require_auth)])


@router.get("/api/notifications")
def get_notifications(include_read: bool = True):
    return list_notifications(include_read=include_read)


@router.post("/api/notifications/read")
def read_notification(req: MarkNotificationReadRequest):
    return mark_notification_read(req.notification_id)
