from __future__ import annotations

from datetime import datetime
from typing import Any

from sqlalchemy import JSON, Boolean, DateTime, Integer, String, Text, func
from sqlalchemy.ext.asyncio import AsyncAttrs, async_sessionmaker, create_async_engine
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column

from .config_new import settings


class Base(AsyncAttrs, DeclarativeBase):
    pass


class Subscription(Base):
    __tablename__ = "subscriptions"

    id: Mapped[str] = mapped_column(String(64), primary_key=True)
    keyword: Mapped[str] = mapped_column(String(255), nullable=False)
    url: Mapped[str] = mapped_column(Text, nullable=False)
    password: Mapped[str] = mapped_column(String(32), default="")
    media_type: Mapped[str] = mapped_column(String(32), default="series")
    enabled: Mapped[bool] = mapped_column(Boolean, default=True)
    completed: Mapped[bool] = mapped_column(Boolean, default=False)
    notify_only: Mapped[bool] = mapped_column(Boolean, default=False)
    
    rules: Mapped[Any] = mapped_column(JSON, default=dict)
    last_check_time: Mapped[Any] = mapped_column(DateTime, nullable=True)
    last_probe: Mapped[Any] = mapped_column(JSON, nullable=True)
    saved_files: Mapped[Any] = mapped_column(JSON, default=list)
    check_history: Mapped[Any] = mapped_column(JSON, default=list)
    no_update_count: Mapped[int] = mapped_column(Integer, default=0)
    
    created_at: Mapped[datetime] = mapped_column(DateTime, server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(DateTime, server_default=func.now(), onupdate=func.now())


class Notification(Base):
    __tablename__ = "notifications"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    level: Mapped[str] = mapped_column(String(32), default="info")
    title: Mapped[str] = mapped_column(String(255), nullable=False)
    message: Mapped[str] = mapped_column(Text, nullable=False)
    read: Mapped[bool] = mapped_column(Boolean, default=False)
    subscription_id: Mapped[Any] = mapped_column(String(64), nullable=True)
    
    created_at: Mapped[datetime] = mapped_column(DateTime, server_default=func.now())


class TransferRecord(Base):
    __tablename__ = "transfer_records"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    subscription_id: Mapped[str] = mapped_column(String(64), nullable=False)
    file_name: Mapped[str] = mapped_column(String(512), nullable=False)
    file_path: Mapped[str] = mapped_column(Text, nullable=False)
    status: Mapped[str] = mapped_column(String(32), default="pending")
    error_message: Mapped[Any] = mapped_column(Text, nullable=True)
    
    quark_saved: Mapped[bool] = mapped_column(Boolean, default=False)
    nas_synced: Mapped[bool] = mapped_column(Boolean, default=False)
    aria2_downloaded: Mapped[bool] = mapped_column(Boolean, default=False)
    
    created_at: Mapped[datetime] = mapped_column(DateTime, server_default=func.now())


engine = create_async_engine(settings.database_url, echo=False)
async_session = async_sessionmaker(engine, expire_on_commit=False)


async def init_db():
    """Initialize database tables."""
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)
