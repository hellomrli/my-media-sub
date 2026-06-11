from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any

from .. import config
from ..utils.cloud_names import CLOUD_TYPE_NAMES

SETTINGS_PATH = Path(os.getenv("SETTINGS_PATH", "./data/settings.json"))

DEFAULT_CLOUD_TYPES = ["quark"]
SUPPORTED_CLOUD_TYPES = [
    "quark", "baidu", "aliyun", "uc", "tianyi", "mobile", "115", "pikpak",
    "xunlei", "123", "magnet", "ed2k", "others",
]

LEGACY_SETTINGS_KEYS = {
    "pansou_base_url",
    "openlist_base_url",
    "openlist_username",
    "openlist_password",
}


def env_bool(name: str, default: str = "false") -> bool:
    return os.getenv(name, default).lower() in {"1", "true", "yes", "on"}


def env_int(name: str, default: int) -> int:
    try:
        return int(os.getenv(name, str(default)))
    except (TypeError, ValueError):
        return default


def default_settings() -> dict[str, Any]:
    return {
        "app_username": config.APP_USERNAME or "admin",
        "app_password": config.APP_PASSWORD or "change-me",
        "cloud_types": DEFAULT_CLOUD_TYPES,
        "check_links": config.CHECK_LINKS,
        "probe_quark_files": config.PROBE_QUARK_FILES,
        "filter_bad_links": config.FILTER_BAD_LINKS,
        "aria2_rpc_url": os.getenv("ARIA2_RPC_URL", ""),
        "aria2_secret": os.getenv("ARIA2_SECRET", ""),
        "aria2_dir": os.getenv("ARIA2_DIR", ""),
        "auto_download_new_subscription_items": env_bool("AUTO_DOWNLOAD_NEW_SUBSCRIPTION_ITEMS"),
        "subscription_scheduler_enabled": env_bool("SUBSCRIPTION_SCHEDULER_ENABLED"),
        "subscription_check_interval_minutes": env_int("SUBSCRIPTION_CHECK_INTERVAL_MINUTES", 60),
        "quark_save_enabled": env_bool("QUARK_SAVE_ENABLED"),
        "quark_save_root": os.getenv("QUARK_SAVE_ROOT", ""),
        "quark_save_movie_dir": os.getenv("QUARK_SAVE_MOVIE_DIR", "/电影"),
        "quark_save_series_dir": os.getenv("QUARK_SAVE_SERIES_DIR", "/连续剧"),
        "quark_save_anime_dir": os.getenv("QUARK_SAVE_ANIME_DIR", "/动画"),
        "custom_categories": [],
        "quark_cookie": os.getenv("QUARK_COOKIE", ""),
        "telegram_bot_token": os.getenv("TELEGRAM_BOT_TOKEN", ""),
        "telegram_chat_id": os.getenv("TELEGRAM_CHAT_ID", ""),
        "bark_url": os.getenv("BARK_URL", ""),
        "gotify_url": os.getenv("GOTIFY_URL", ""),
        "gotify_token": os.getenv("GOTIFY_TOKEN", ""),
        "pushplus_token": os.getenv("PUSHPLUS_TOKEN", ""),
        "serverchan_key": os.getenv("SERVERCHAN_KEY", ""),
        "push_on_update": env_bool("PUSH_ON_UPDATE", "true"),
        "push_on_failed": env_bool("PUSH_ON_FAILED", "true"),
        "push_on_completed": env_bool("PUSH_ON_COMPLETED", "true"),
        "push_on_save": env_bool("PUSH_ON_SAVE", "true"),
        "push_silent": env_bool("PUSH_SILENT", "false"),
        "nas_sync_enabled": env_bool("NAS_SYNC_ENABLED"),
        "nas_sync_source": os.getenv("NAS_SYNC_SOURCE", ""),
        "nas_sync_target": os.getenv("NAS_SYNC_TARGET", ""),
        "wecom_bot_url": os.getenv("WECOM_BOT_URL", ""),
        "wxpusher_app_token": os.getenv("WXPUSHER_APP_TOKEN", ""),
        "wxpusher_uids": os.getenv("WXPUSHER_UIDS", ""),
    }


class SettingsStore:
    def __init__(self, path: Path = SETTINGS_PATH):
        self.path = path
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._settings = default_settings()
        self.load()

    def load(self):
        if self.path.exists():
            try:
                data = json.loads(self.path.read_text())
                if isinstance(data, dict):
                    for key in LEGACY_SETTINGS_KEYS:
                        data.pop(key, None)
                    self._settings.update(data)
            except Exception:
                # Keep defaults if settings file is broken.
                pass
        else:
            self.save()

    def save(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(".tmp")
        tmp.write_text(json.dumps(self._settings, ensure_ascii=False, indent=2))
        tmp.replace(self.path)

    def get(self) -> dict[str, Any]:
        return dict(self._settings)

    def public(self) -> dict[str, Any]:
        data = self.get()
        for key in LEGACY_SETTINGS_KEYS:
            data.pop(key, None)
        for key in ("app_password", "aria2_secret", "quark_cookie", "wxpusher_app_token"):
            data[f"{key}_configured"] = bool(data.get(key))
            data[key] = ""
        data["supported_cloud_types"] = SUPPORTED_CLOUD_TYPES
        data["cloud_type_names"] = CLOUD_TYPE_NAMES
        data["app_name"] = "Lain 的媒体订阅"
        return data

    def update_secret(self, key: str, value: str) -> None:
        if key not in {"quark_cookie", "aria2_secret", "app_password"}:
            return
        self._settings[key] = value
        self.save()

    def update(self, patch: dict[str, Any]) -> dict[str, Any]:
        allowed = set(default_settings())
        for key, value in patch.items():
            if key not in allowed:
                continue
            if key == "cloud_types":
                if not isinstance(value, list):
                    continue
                value = [v for v in value if v in SUPPORTED_CLOUD_TYPES]
                if not value:
                    value = DEFAULT_CLOUD_TYPES
            elif key == "subscription_check_interval_minutes":
                try:
                    value = max(int(str(value)), 5)
                except (TypeError, ValueError):
                    value = 60
            elif key in {"aria2_rpc_url"} and isinstance(value, str):
                value = value.strip().rstrip("/")
            self._settings[key] = value
        self.save()
        return self.get()


settings_store = SettingsStore()
