from __future__ import annotations

from fastapi import APIRouter, Depends, HTTPException

from ..auth import require_auth
from ..schemas.requests import DownloadRequest
from ..services.download_service import download_with_aria2, test_aria2

router = APIRouter(dependencies=[Depends(require_auth)])


@router.post("/api/aria2/test")
def aria2_test():
    return test_aria2()


@router.post("/api/download/aria2")
def aria2_download(req: DownloadRequest):
    try:
        return download_with_aria2(req.chat_id, req.index, req.url, req.dir)
    except LookupError as e:
        raise HTTPException(status_code=404, detail=str(e)) from e
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
