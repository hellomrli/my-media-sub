from __future__ import annotations

from fastapi import APIRouter, Depends

from ..auth import require_auth
from ..schemas.requests import (
    QuarkDriveCopyRequest,
    QuarkDriveCreateFolderRequest,
    QuarkDriveDeleteRequest,
    QuarkDriveDownloadRequest,
    QuarkDriveListRequest,
    QuarkDriveMoveRequest,
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


@router.post("/api/quark-drive/download")
def download_file(req: QuarkDriveDownloadRequest):
    result = quark_drive_service.download_from_quark(req.fid, req.file_name, req.dir)
    if not result.get("ok"):
        from fastapi import HTTPException
        raise HTTPException(status_code=400, detail=result.get("message"))
    return result


@router.post("/api/quark-drive/move")
def move_items(req: QuarkDriveMoveRequest):
    return quark_drive_service.move_items(req.fids, req.target_fid)


@router.post("/api/quark-drive/copy")
def copy_items(req: QuarkDriveCopyRequest):
    return quark_drive_service.copy_items(req.fids, req.target_fid)


@router.post("/api/quark-drive/resolve-path")
def resolve_path(req: dict):
    path = req.get("path", "")
    return quark_drive_service.resolve_path_to_fid(path)
