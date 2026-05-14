from __future__ import annotations

import json
import os
import time
from pathlib import Path
from typing import Any
from uuid import uuid4

from .episode_utils import normalize_probe_files, summarize_episodes

SUBSCRIPTIONS_PATH = Path(os.getenv("SUBSCRIPTIONS_PATH", "/data/subscriptions.json"))

MEDIA_TYPE_LABELS = {
    "series": "连续剧",
    "anime": "动画",
}


def default_rules() -> dict[str, Any]:
    return {
        "include_keywords": [],
        "exclude_keywords": ["预告", "花絮", "解说", "彩蛋", "trailer", "preview"],
        "match_regex": "",
        "rename_template": "",
        "only_latest": False,
        "auto_download": False,
        "notify_on_update": True,
        "notify_on_invalid": True,
        "check_interval_minutes": 60,
        "check_weekdays": [],
        "finish_after_episode": None,
    }


def migrate_subscription(sub: dict[str, Any]) -> dict[str, Any]:
    sub.setdefault("media_type", "series")
    sub.setdefault("season", 1)
    sub.setdefault("total_episode_number", None)
    sub.setdefault("source_group", sub.get("source_title") or "")
    sub.setdefault("enabled", True)
    sub.setdefault("completed", False)
    sub.setdefault("status", "active")
    sub.setdefault("invalid_since", None)
    sub.setdefault("last_error", "")
    sub.setdefault("last_new_files", [])
    sub.setdefault("last_new_episodes", [])
    sub.setdefault("last_check_summary", "")
    sub.setdefault("check_history", [])
    rules = default_rules()
    rules.update(sub.get("rules") or {})
    sub["rules"] = rules
    sub.setdefault("known_file_keys", [])
    if not sub["known_file_keys"] and sub.get("known_files"):
        sub["known_file_keys"] = [f"name:{name}" for name in sub.get("known_files") or []]
    probe = sub.get("last_probe") or {}
    normalized = normalize_probe_files(
        probe.get("files") or [],
        rules.get("include_keywords"),
        rules.get("exclude_keywords"),
        rules.get("match_regex") or "",
    )
    summary = summarize_episodes(normalized)
    sub.setdefault("current_episode_number", summary["current_episode_number"])
    return sub


class SubscriptionStore:
    def __init__(self, path: Path = SUBSCRIPTIONS_PATH):
        self.path = path
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._items: list[dict[str, Any]] = []
        self.load()

    def load(self):
        if self.path.exists():
            try:
                data = json.loads(self.path.read_text())
                self._items = [migrate_subscription(x) for x in data] if isinstance(data, list) else []
            except Exception:
                self._items = []
        else:
            self.save()

    def save(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(".tmp")
        tmp.write_text(json.dumps(self._items, ensure_ascii=False, indent=2))
        tmp.replace(self.path)

    def list(self) -> list[dict[str, Any]]:
        return [migrate_subscription(dict(x)) for x in self._items]

    def get(self, sub_id: str) -> dict[str, Any] | None:
        sub = next((x for x in self._items if x.get("id") == sub_id), None)
        return migrate_subscription(sub) if sub else None

    def create_from_item(self, keyword: str, item: dict[str, Any], notify_only: bool = True, media_type: str = "series") -> dict[str, Any]:
        probe = item.get("probe") or {}
        rules = default_rules()
        normalized = normalize_probe_files(probe.get("files") or [], rules["include_keywords"], rules["exclude_keywords"], rules["match_regex"])
        summary = summarize_episodes(normalized)
        known_names = sorted({f.get("name") for f in normalized if f.get("name")})
        known_keys = sorted({f.get("file_key") for f in normalized if f.get("file_key")})
        now = int(time.time())
        sub = {
            "id": uuid4().hex[:12],
            "title": keyword or item.get("title") or "未命名订阅",
            "source_title": item.get("title") or "",
            "media_type": media_type,
            "season": 1,
            "current_episode_number": summary["current_episode_number"],
            "total_episode_number": None,
            "source_group": item.get("source") or "",
            "cloud_type": item.get("cloud_type") or "quark",
            "url": item.get("url"),
            "password": item.get("password") or "",
            "known_files": known_names,
            "known_file_keys": known_keys,
            "known_episodes": summary["episodes"],
            "last_probe": probe,
            "notify_only": notify_only,
            "enabled": True,
            "completed": False,
            "rules": rules,
            "created_at": now,
            "updated_at": now,
            "last_checked_at": now,
            "last_new_files": [],
            "last_new_episodes": [],
            "last_check_summary": f"创建订阅，已记录 {len(known_keys)} 个文件，当前进度 {summary['current_episode_number']}。",
            "check_history": [],
            "status": "active",
            "invalid_since": None,
            "last_error": "",
        }
        self._items.append(sub)
        self.save()
        return sub

    def update(self, sub_id: str, patch: dict[str, Any]) -> dict[str, Any] | None:
        sub = next((x for x in self._items if x.get("id") == sub_id), None)
        if not sub:
            return None
        allowed = {"title", "season", "total_episode_number", "enabled", "completed", "source_group", "notify_only"}
        for key in allowed:
            if key in patch:
                sub[key] = patch[key]
        if "rules" in patch and isinstance(patch["rules"], dict):
            rules = default_rules()
            rules.update(sub.get("rules") or {})
            for key in rules:
                if key in patch["rules"]:
                    rules[key] = patch["rules"][key]
            sub["rules"] = rules
        sub["updated_at"] = int(time.time())
        self.save()
        return migrate_subscription(sub)

    def update_check(self, sub_id: str, probe: dict[str, Any]) -> tuple[dict[str, Any] | None, list[str], bool]:
        sub = next((x for x in self._items if x.get("id") == sub_id), None)
        if not sub:
            return None, [], False
        sub = migrate_subscription(sub)
        was_invalid = sub.get("status") == "invalid"
        is_invalid = probe.get("state") in {"bad", "invalid_url", "locked"} or (probe.get("ok") is False and probe.get("state") in {"bad", "invalid_url"})
        rules = sub.get("rules") or default_rules()
        normalized = normalize_probe_files(
            probe.get("files") or [],
            rules.get("include_keywords"),
            rules.get("exclude_keywords"),
            rules.get("match_regex") or "",
        )
        summary = summarize_episodes(normalized)
        known_keys = set(sub.get("known_file_keys") or [])
        new_items = [f for f in normalized if f.get("file_key") not in known_keys]
        if rules.get("only_latest") and new_items:
            max_ep = max([f.get("episode") or 0 for f in new_items])
            new_items = [f for f in new_items if (f.get("episode") or 0) == max_ep]
        new_files = [f.get("name") for f in new_items if f.get("name")]
        new_episodes = sorted({f.get("episode") for f in new_items if f.get("episode") is not None})
        all_keys = sorted(known_keys | {f.get("file_key") for f in normalized if f.get("file_key")})
        all_names = sorted({*(sub.get("known_files") or []), *[f.get("name") for f in normalized if f.get("name")]})
        sub["known_file_keys"] = all_keys
        sub["known_files"] = all_names
        sub["known_episodes"] = sorted(set(sub.get("known_episodes") or []) | set(summary["episodes"]))
        sub["current_episode_number"] = max(sub.get("current_episode_number") or 0, summary["current_episode_number"] or 0)
        sub["last_probe"] = probe
        sub["last_new_files"] = new_files
        sub["last_new_episodes"] = new_episodes
        sub["last_checked_at"] = int(time.time())
        sub["updated_at"] = int(time.time())
        sub["status"] = "invalid" if is_invalid else "completed" if sub.get("completed") else "active"
        sub["invalid_since"] = sub.get("invalid_since") or int(time.time()) if is_invalid else None
        sub["last_error"] = probe.get("message") or "" if is_invalid else ""
        if rules.get("finish_after_episode") and sub["current_episode_number"] >= int(rules["finish_after_episode"]):
            sub["completed"] = True
            sub["status"] = "completed"
        sub["last_check_summary"] = f"匹配 {len(normalized)} 个文件，新增 {len(new_files)} 个，当前进度 {sub['current_episode_number']}。"
        sub.setdefault("check_history", [])
        sub["check_history"].append({
            "time": int(time.time()),
            "state": sub["status"],
            "matched_count": len(normalized),
            "new_files": new_files,
            "new_episodes": new_episodes,
            "summary": sub["last_check_summary"],
        })
        sub["check_history"] = sub["check_history"][-30:]
        self.save()
        became_invalid = is_invalid and not was_invalid
        return sub, new_files, became_invalid

    def delete(self, sub_id: str) -> bool:
        before = len(self._items)
        self._items = [x for x in self._items if x.get("id") != sub_id]
        changed = len(self._items) != before
        if changed:
            self.save()
        return changed


subscription_store = SubscriptionStore()
