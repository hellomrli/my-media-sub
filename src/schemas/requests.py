from __future__ import annotations

from typing import Any

from pydantic import BaseModel, Field


class SearchRequest(BaseModel):
    keyword: str = Field(..., min_length=1)
    chat_id: str = "default"
    limit: int = Field(default=50, ge=1, le=100)
    cloud_types: list[str] | None = None
    check_links: bool | None = None
    probe_files: bool | None = None
    filter_bad_links: bool | None = None


class SelectRequest(BaseModel):
    chat_id: str = "default"
    index: int = Field(..., ge=1)


class SubscribeRequest(BaseModel):
    chat_id: str = "default"
    index: int = Field(..., ge=1)
    media_type: str = "series"  # movie | series | anime
    notify_only: bool = True


class CheckSubscriptionRequest(BaseModel):
    subscription_id: str


class UpdateSubscriptionRequest(BaseModel):
    subscription_id: str
    title: str | None = None
    media_type: str | None = None
    season: int | None = None
    total_episode_number: int | None = None
    enabled: bool | None = None
    completed: bool | None = None
    source_group: str | None = None
    notify_only: bool | None = None
    rules: dict[str, Any] | None = None


class PlanSubscriptionRequest(BaseModel):
    subscription_id: str
    files: list[dict[str, Any]] | None = None
    target_existing_files: list[str] | None = None
    target_dir_exists: bool | None = None
    rules: dict[str, Any] | None = None


class DeleteSubscriptionRequest(BaseModel):
    subscription_id: str


class MarkNotificationReadRequest(BaseModel):
    notification_id: str | None = None


class DownloadRequest(BaseModel):
    chat_id: str = "default"
    index: int | None = Field(default=None, ge=1)
    url: str | None = None
    dir: str | None = None


class WechatMessageRequest(BaseModel):
    chat_id: str = "default"
    text: str = Field(..., min_length=1)


class SettingsUpdateRequest(BaseModel):
    app_username: str | None = None
    app_password: str | None = None
    cloud_types: list[str] | None = None
    check_links: bool | None = None
    probe_quark_files: bool | None = None
    filter_bad_links: bool | None = None
    aria2_rpc_url: str | None = None
    aria2_secret: str | None = None
    aria2_dir: str | None = None
    auto_download_new_subscription_items: bool | None = None
    subscription_scheduler_enabled: bool | None = None
    subscription_check_interval_minutes: int | None = Field(default=None, ge=5)
    quark_save_enabled: bool | None = None
    quark_cookie: str | None = None
    quark_save_root: str | None = None
    quark_save_movie_dir: str | None = None
    quark_save_series_dir: str | None = None
    quark_save_anime_dir: str | None = None
    custom_categories: list[dict] | None = None
    nas_sync_enabled: bool | None = None
    nas_sync_source: str | None = None
    nas_sync_target: str | None = None
    wecom_bot_url: str | None = None
    wxpusher_app_token: str | None = None
    wxpusher_uids: str | None = None
    telegram_bot_token: str | None = None
    telegram_chat_id: str | None = None
    bark_url: str | None = None
    gotify_url: str | None = None
    gotify_token: str | None = None
    pushplus_token: str | None = None
    serverchan_key: str | None = None
    push_on_update: bool | None = None
    push_on_failed: bool | None = None
    push_on_completed: bool | None = None
    push_on_save: bool | None = None
    push_silent: bool | None = None


class QuarkDriveListRequest(BaseModel):
    parent_fid: str = "0"


class QuarkDriveCreateFolderRequest(BaseModel):
    parent_fid: str = "0"
    name: str = Field(..., min_length=1)


class QuarkDriveRenameRequest(BaseModel):
    fid: str = Field(..., min_length=1)
    name: str = Field(..., min_length=1)


class QuarkDriveDeleteRequest(BaseModel):
    fids: list[str] = Field(default_factory=list)


class QuarkDriveDownloadRequest(BaseModel):
    fid: str = Field(..., min_length=1)
    file_name: str | None = None
    dir: str | None = None


class QuarkDriveMoveRequest(BaseModel):
    fids: list[str] = Field(default_factory=list)
    target_fid: str = "0"


class QuarkDriveCopyRequest(BaseModel):
    fids: list[str] = Field(default_factory=list)
    target_fid: str = "0"
