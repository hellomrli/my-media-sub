from __future__ import annotations

from fastapi import APIRouter, Depends, HTTPException

from ..auth import require_auth
from ..schemas.requests import WechatMessageRequest
from ..services.wechat_service import handle_wechat_message

router = APIRouter(dependencies=[Depends(require_auth)])


@router.post("/api/wechat/message")
def wechat_message(req: WechatMessageRequest):
    try:
        return handle_wechat_message(req.chat_id, req.text)
    except LookupError as e:
        raise HTTPException(status_code=404, detail=str(e)) from e
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
