from __future__ import annotations

from fastapi import APIRouter, Depends

from ..auth import require_auth
from ..schemas.requests import (
    QuarkDriveCreateFolderRequest,
    QuarkDriveDeleteRequest,
    QuarkDriveListRequest,
    QuarkDriveRenameRequest,
)
from ..services import quark_drive_service

router = APIRouter(dependencies=[Depends(require_auth)])


@router.post("/api/quark-drive/list")
def list_drive(req: QuarkDriveListRequest):
    return quark_drive_service.list_drive(req.parent_fid)


@router.post("/api/quark-drive/folder")
def create_folder(req: QuarkDriveCreateFolderRequest):
    return quark_drive_service.create_folder(req.parent_fid, req.name)


@router.post("/api/quark-drive/rename")
def rename_item(req: QuarkDriveRenameRequest):
    return quark_drive_service.rename_item(req.fid, req.name)


@router.post("/api/quark-drive/delete")
def delete_items(req: QuarkDriveDeleteRequest):
    return quark_drive_service.delete_items(req.fids)
