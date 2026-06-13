from __future__ import annotations

from fastapi import APIRouter, Depends, HTTPException

from ..auth import require_auth
from ..schemas.requests import SearchRequest, SelectRequest
from ..services.search_service import search_media, select_result

router = APIRouter(dependencies=[Depends(require_auth)])


@router.post("/api/search")
def search(req: SearchRequest):
    return search_media(
        keyword=req.keyword,
        chat_id=req.chat_id,
        limit=req.limit,
        cloud_types=req.cloud_types,
        check_links=req.check_links,
        probe_files=req.probe_files,
        filter_bad_links=req.filter_bad_links,
    )


@router.post("/api/select")
def select(req: SelectRequest):
    try:
        return select_result(req.chat_id, req.index)
    except LookupError as e:
        raise HTTPException(status_code=404, detail=str(e)) from e
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
