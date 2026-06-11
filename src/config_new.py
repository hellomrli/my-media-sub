from __future__ import annotations

from pathlib import Path
from typing import Any

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore",
    )

    # App
    app_username: str = Field(default="", description="HTTP Basic Auth username")
    app_password: str = Field(default="", description="HTTP Basic Auth password")
    
    # Database
    database_url: str = Field(default="sqlite+aiosqlite:///./data/app.db", description="Database URL")
    
    # Search
    check_links: bool = Field(default=True, description="Check link validity")
    probe_quark_files: bool = Field(default=True, description="Probe Quark share files")
    filter_bad_links: bool = Field(default=True, description="Filter out invalid links")
    
    # Aria2
    aria2_rpc_url: str = Field(default="", description="Aria2 RPC URL")
    aria2_secret: str = Field(default="", description="Aria2 RPC secret")
    aria2_download_dir: str = Field(default="/downloads", description="Aria2 download directory")
    auto_download_new_subscription_items: bool = Field(default=False, description="Auto download new subscription items")
    
    # Quark
    quark_cookie: str = Field(default="", description="Quark cookie for API access")
    quark_save_enabled: bool = Field(default=False, description="Enable auto save to Quark")
    quark_save_root: str = Field(default="/媒体/连续剧", description="Quark save root directory")
    
    # NAS Sync
    nas_sync_enabled: bool = Field(default=False, description="Enable NAS sync")
    nas_sync_source: str = Field(default="", description="NAS sync source path (local mount)")
    nas_sync_target: str = Field(default="", description="NAS sync target path")
    
    # Scheduler
    subscription_scheduler_enabled: bool = Field(default=False, description="Enable subscription scheduler")
    subscription_check_interval_minutes: int = Field(default=60, description="Subscription check interval in minutes")
    
    # Notifications
    telegram_bot_token: str = Field(default="", description="Telegram bot token for notifications")
    telegram_chat_id: str = Field(default="", description="Telegram chat ID for notifications")
    notification_level: str = Field(default="info", description="Notification level: info/success/warning/error")
    
    # Quality preferences
    preferred_quality_order: list[str] = Field(
        default=["4K", "2160p", "1080p", "720p", "480p"],
        description="Preferred quality order for deduplication"
    )
    
    # Auto completion
    auto_complete_after_no_updates: int = Field(default=5, description="Mark subscription completed after N checks with no updates")


settings = Settings()
