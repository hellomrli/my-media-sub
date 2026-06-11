#!/usr/bin/env python3
"""
Migrate from JSON storage to SQLite database.

Usage:
    python -m src.scripts.migrate_to_db
"""

import asyncio
import json
import sys
from pathlib import Path


async def migrate():
    """Main migration function."""
    print("🔄 Starting migration from JSON to SQLite...")
    
    # Import after adding project to path
    from src.database import init_db, async_session, Subscription, Notification
    from datetime import datetime
    
    # Initialize database
    print("📦 Initializing database...")
    await init_db()
    
    # Migrate subscriptions
    subscriptions_file = Path("data/subscriptions.json")
    if subscriptions_file.exists():
        print(f"📂 Found subscriptions file: {subscriptions_file}")
        with open(subscriptions_file, "r", encoding="utf-8") as f:
            subscriptions_data = json.load(f)
        
        async with async_session() as session:
            for sub_data in subscriptions_data:
                # Convert old format to new
                sub = Subscription(
                    id=sub_data.get("id"),
                    keyword=sub_data.get("keyword", ""),
                    url=sub_data.get("url", ""),
                    password=sub_data.get("password", ""),
                    media_type=sub_data.get("media_type", "series"),
                    enabled=sub_data.get("enabled", True),
                    completed=sub_data.get("completed", False),
                    notify_only=sub_data.get("notify_only", False),
                    rules=sub_data.get("rules", {}),
                    last_probe=sub_data.get("last_probe"),
                    saved_files=sub_data.get("saved_files", []),
                    check_history=sub_data.get("check_history", []),
                    no_update_count=0,
                )
                session.add(sub)
                print(f"  ✅ Migrated subscription: {sub.keyword}")
            
            await session.commit()
        print(f"✅ Migrated {len(subscriptions_data)} subscriptions")
    else:
        print("⚠️  No subscriptions file found, skipping...")
    
    # Migrate notifications
    notifications_file = Path("data/notifications.json")
    if notifications_file.exists():
        print(f"📂 Found notifications file: {notifications_file}")
        with open(notifications_file, "r", encoding="utf-8") as f:
            notifications_data = json.load(f)
        
        async with async_session() as session:
            for notif_data in notifications_data:
                notif = Notification(
                    level=notif_data.get("level", "info"),
                    title=notif_data.get("title", ""),
                    message=notif_data.get("message", ""),
                    read=notif_data.get("read", False),
                    subscription_id=notif_data.get("subscription_id"),
                )
                session.add(notif)
            
            await session.commit()
        print(f"✅ Migrated {len(notifications_data)} notifications")
    else:
        print("⚠️  No notifications file found, skipping...")
    
    # Migrate settings to .env
    settings_file = Path("data/settings.json")
    env_file = Path(".env")
    
    if settings_file.exists() and not env_file.exists():
        print(f"📂 Found settings file: {settings_file}")
        with open(settings_file, "r", encoding="utf-8") as f:
            settings_data = json.load(f)
        
        env_lines = [
            "# Migrated from settings.json",
            f"QUARK_COOKIE={settings_data.get('quark_cookie', '')}",
            f"QUARK_SAVE_ENABLED={str(settings_data.get('quark_save_enabled', False)).lower()}",
            f"QUARK_SAVE_ROOT={settings_data.get('quark_save_root', '/媒体/连续剧')}",
            f"ARIA2_RPC_URL={settings_data.get('aria2_rpc_url', '')}",
            f"ARIA2_SECRET={settings_data.get('aria2_secret', '')}",
            f"ARIA2_DOWNLOAD_DIR={settings_data.get('aria2_download_dir', '/downloads')}",
            f"NAS_SYNC_ENABLED={str(settings_data.get('nas_sync_enabled', False)).lower()}",
            f"NAS_SYNC_SOURCE={settings_data.get('nas_sync_source', '')}",
            f"NAS_SYNC_TARGET={settings_data.get('nas_sync_target', '')}",
            f"SUBSCRIPTION_SCHEDULER_ENABLED={str(settings_data.get('subscription_scheduler_enabled', False)).lower()}",
            f"SUBSCRIPTION_CHECK_INTERVAL_MINUTES={settings_data.get('subscription_check_interval_minutes', 60)}",
        ]
        
        with open(env_file, "w", encoding="utf-8") as f:
            f.write("\n".join(env_lines))
        
        print(f"✅ Migrated settings to {env_file}")
    else:
        if env_file.exists():
            print("⚠️  .env already exists, skipping settings migration")
        else:
            print("⚠️  No settings file found, skipping...")
    
    print("\n🎉 Migration completed!")
    print("\n📋 Next steps:")
    print("  1. Review the migrated data in data/app.db")
    print("  2. Update your .env file with any missing configuration")
    print("  3. Backup your old JSON files: mv data/*.json data/backup/")
    print("  4. Restart the application")


if __name__ == "__main__":
    asyncio.run(migrate())
