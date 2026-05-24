from __future__ import annotations

from fastapi import APIRouter, Depends

from ..auth import require_auth
from ..schemas.requests import SettingsUpdateRequest
from ..services import settings_service

router = APIRouter(dependencies=[Depends(require_auth)])


@router.get("/api/settings")
def get_settings():
    return settings_service.get_settings()


@router.post("/api/settings")
def update_settings(req: SettingsUpdateRequest):
    return settings_service.update_settings(req.model_dump(exclude_unset=True))


@router.get("/api/cloud-types")
def get_cloud_types():
    return settings_service.get_cloud_types()
