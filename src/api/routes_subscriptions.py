from __future__ import annotations

from fastapi import APIRouter, Depends, HTTPException

from ..auth import require_auth
from ..schemas.requests import (
    CheckSubscriptionRequest,
    DeleteSubscriptionRequest,
    PlanSubscriptionRequest,
    SubscribeRequest,
    UpdateSubscriptionRequest,
)
from ..services import subscription_service
from ..stores.subscription_store import subscription_store

router = APIRouter(dependencies=[Depends(require_auth)])


@router.get("/api/subscriptions")
def list_subscriptions():
    return {"subscriptions": subscription_store.list()}


@router.post("/api/subscriptions")
def create_subscription(req: SubscribeRequest):
    try:
        sub = subscription_service.create_subscription_from_selection(
            req.chat_id,
            req.index,
            media_type=req.media_type,
            notify_only=req.notify_only,
        )
    except LookupError as e:
        raise HTTPException(status_code=404, detail=str(e)) from e
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return {"subscription": sub}


@router.post("/api/subscriptions/update")
def update_subscription(req: UpdateSubscriptionRequest):
    patch = req.model_dump(exclude_unset=True)
    sub_id = patch.pop("subscription_id")
    updated = subscription_service.update_subscription(sub_id, patch)
    if not updated:
        raise HTTPException(status_code=404, detail="订阅不存在。")
    return {"subscription": updated}


@router.post("/api/subscriptions/check")
def check_subscription(req: CheckSubscriptionRequest):
    try:
        updated, new_files, became_invalid = subscription_service.check_subscription(req.subscription_id)
    except LookupError as e:
        raise HTTPException(status_code=404, detail=str(e)) from e
    return {"subscription": updated, "new_files": new_files, "became_invalid": became_invalid}


@router.post("/api/subscriptions/plan")
def plan_subscription(req: PlanSubscriptionRequest):
    try:
        plan = subscription_service.plan_subscription(
            req.subscription_id,
            files=req.files,
            rules=req.rules,
            target_existing_files=req.target_existing_files,
            target_dir_exists=req.target_dir_exists,
        )
    except LookupError as e:
        raise HTTPException(status_code=404, detail=str(e)) from e
    return {"plan": plan}


@router.post("/api/subscriptions/check-all")
def check_all_subscriptions():
    return {"results": subscription_service.check_all_subscriptions()}


@router.post("/api/subscriptions/delete")
def delete_subscription(req: DeleteSubscriptionRequest):
    return {"deleted": subscription_store.delete(req.subscription_id)}
