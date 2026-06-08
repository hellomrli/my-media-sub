from __future__ import annotations

from ..stores.settings_store import settings_store
from ..utils.cloud_names import CLOUD_TYPE_NAMES


def get_settings() -> dict:
    data = settings_store.public()
    from .scheduler_service import scheduler_state
    data["subscription_scheduler"] = scheduler_state()
    return data


def update_settings(patch: dict) -> dict:
    settings_store.update(patch)
    return settings_store.public()


def get_cloud_types() -> dict:
    return {"cloud_types": CLOUD_TYPE_NAMES}


def health_payload() -> dict:
    settings = settings_store.get()
    return {
        "status": "ok",
        "pansou_base_url": settings.get("pansou_base_url"),
        "openlist_base_url": settings.get("openlist_base_url"),
        "auth_enabled": bool(settings.get("app_username") and settings.get("app_password")),
        "check_links": settings.get("check_links"),
        "probe_quark_files": settings.get("probe_quark_files"),
        "filter_bad_links": settings.get("filter_bad_links"),
        "app_name": "Lain 的媒体订阅",
    }
