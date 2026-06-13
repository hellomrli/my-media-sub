from __future__ import annotations

import json
import os
import time
from pathlib import Path
from typing import Any
from uuid import uuid4

from ..services.transfer_rule_service import (
    build_transfer_plan,
    default_transfer_rules,
    normalize_rules,
    summarize_rules,
)

SUBSCRIPTIONS_PATH = Path(os.getenv("SUBSCRIPTIONS_PATH", "./data/subscriptions.json"))

MEDIA_TYPE_LABELS = {
    "movie": "电影",
    "series": "连续剧",
    "anime": "动画",
    # 自定义分类通过 custom_ 前缀标识
}


def default_rules() -> dict[str, Any]:
    return default_transfer_rules()


def subscription_view(sub: dict[str, Any]) -> dict[str, Any]:
    item = dict(sub)
    item["rules"] = normalize_rules(item.get("rules"))
    item.setdefault("media_type", "series")
    item.setdefault("season", 1)
    item.setdefault("total_episode_number", None)
    item.setdefault("source_group", "")
    item.setdefault("enabled", True)
    item.setdefault("completed", False)
    item.setdefault("status", "active")
    item.setdefault("invalid_since", None)
    item.setdefault("last_error", "")
    item.setdefault("last_new_files", [])
    item.setdefault("last_new_episodes", [])
    item.setdefault("last_check_summary", "")
    item.setdefault("last_plan_summary", "")
    item.setdefault("check_history", [])
    item.setdefault("known_file_keys", [])
    item.setdefault("known_files", [])
    item.setdefault("transferred_file_keys", [])
    item.setdefault("transferred_files", [])
    item["rule_summary"] = summarize_rules(item["rules"])
    return item


class SubscriptionStore:
    """Persistence boundary for subscription models.

    Store owns JSON loading/saving and model-shaped mutations only. Rule matching,
    rename decisions and transfer planning live in transfer_rule_service; API route
    adapters and network probing live elsewhere.
    """

    def __init__(self, path: Path = SUBSCRIPTIONS_PATH):
        self.path = path
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._items: list[dict[str, Any]] = []
        self.load()

    def load(self):
        if self.path.exists():
            try:
                data = json.loads(self.path.read_text())
                self._items = [subscription_view(x) for x in data] if isinstance(data, list) else []
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
        return [subscription_view(x) for x in self._items]

    def get(self, sub_id: str) -> dict[str, Any] | None:
        sub = next((x for x in self._items if x.get("id") == sub_id), None)
        return subscription_view(sub) if sub else None

    def create_from_item(self, keyword: str, item: dict[str, Any], notify_only: bool = True, media_type: str = "series") -> dict[str, Any]:
        probe = item.get("probe") or {}
        rules = default_rules()
        if keyword:
            rules["target_dir"] = f"/{keyword}"
            rules["rename_template"] = f"{keyword}.S01E{{}}"
        plan = build_transfer_plan({"title": keyword or item.get("title") or "未命名订阅", "season": 1, "rules": rules}, probe.get("files") or [])
        now = int(time.time())
        known_names = sorted({x["source_name"] for x in plan["items"] if x.get("source_name")})
        known_keys = sorted({x["file_key"] for x in plan["items"] if x.get("file_key")})
        sub = {
            "id": uuid4().hex[:12],
            "title": keyword or item.get("title") or "未命名订阅",
            "source_title": item.get("title") or "",
            "media_type": media_type,
            "season": 1,
            "current_episode_number": plan["current_episode_number"],
            "total_episode_number": None,
            "source_group": item.get("source") or "",
            "cloud_type": item.get("cloud_type") or "quark",
            "url": item.get("url"),
            "password": item.get("password") or "",
            "known_files": known_names,
            "known_file_keys": known_keys,
            "known_episodes": plan["episodes"],
            "transferred_files": [],
            "transferred_file_keys": [],
            "last_probe": probe,
            "last_plan_summary": plan["summary"],
            "notify_only": notify_only,
            "enabled": True,
            "completed": False,
            "rules": rules,
            "created_at": now,
            "updated_at": now,
            "last_checked_at": now,
            "last_new_files": [x["source_name"] for x in plan["transfers"]],
            "last_new_episodes": sorted({x.get("episode") for x in plan["transfers"] if x.get("episode") is not None}),
            "last_check_summary": f"创建订阅，规划转存 {plan['transfer_count']} 个，当前进度 {plan['current_episode_number']}。",
            "check_history": [],
            "status": "active",
            "invalid_since": None,
            "last_error": "",
        }
        self._items.append(sub)
        self.save()
        return subscription_view(sub)

    def update(self, sub_id: str, patch: dict[str, Any]) -> dict[str, Any] | None:
        sub = next((x for x in self._items if x.get("id") == sub_id), None)
        if not sub:
            return None
        allowed = {"title", "media_type", "season", "total_episode_number", "enabled", "completed", "source_group", "notify_only"}
        for key in allowed:
            if key in patch:
                sub[key] = patch[key]
        if "rules" in patch and isinstance(patch["rules"], dict):
            rules = normalize_rules(patch["rules"])
            sub["rules"] = rules
            plan = build_transfer_plan(subscription_view(sub), (sub.get("last_probe") or {}).get("files") or [])
            sub["last_plan_summary"] = plan["summary"]
        sub["updated_at"] = int(time.time())
        self.save()
        return subscription_view(sub)

    def update_check(self, sub_id: str, probe: dict[str, Any], plan: dict[str, Any] | None = None) -> tuple[dict[str, Any] | None, list[str], bool]:
        sub = next((x for x in self._items if x.get("id") == sub_id), None)
        if not sub:
            return None, [], False
        sub_view = subscription_view(sub)
        was_invalid = sub_view.get("status") == "invalid"
        is_invalid = probe.get("state") in {"bad", "invalid_url", "locked"} or (probe.get("ok") is False and probe.get("state") in {"bad", "invalid_url"})
        plan = plan or build_transfer_plan(sub_view, probe.get("files") or [])
        transfers = plan.get("transfers") or []
        new_files = [x.get("source_name") for x in transfers if x.get("source_name")]
        new_episodes = sorted({x.get("episode") for x in transfers if x.get("episode") is not None})
        known_keys = sorted({*(sub.get("known_file_keys") or []), *[x.get("file_key") for x in plan.get("items", []) if x.get("file_key")]})
        known_names = sorted({*(sub.get("known_files") or []), *[x.get("source_name") for x in plan.get("items", []) if x.get("source_name")]})
        sub["known_file_keys"] = known_keys
        sub["known_files"] = known_names
        sub["known_episodes"] = sorted(set(sub.get("known_episodes") or []) | set(plan.get("episodes") or []))
        sub["current_episode_number"] = max(sub.get("current_episode_number") or 0, plan.get("current_episode_number") or 0)
        sub["last_probe"] = probe
        sub["last_plan_summary"] = plan.get("summary") or ""
        sub["last_new_files"] = new_files
        sub["last_new_episodes"] = new_episodes
        sub["last_checked_at"] = int(time.time())
        sub["updated_at"] = int(time.time())
        sub["status"] = "invalid" if is_invalid else "completed" if sub.get("completed") else "active"
        sub["invalid_since"] = sub.get("invalid_since") or int(time.time()) if is_invalid else None
        sub["last_error"] = probe.get("message") or "" if is_invalid else ""
        rules = normalize_rules(sub.get("rules"))
        if rules.get("finish_after_episode") and sub["current_episode_number"] >= int(rules["finish_after_episode"]):
            sub["completed"] = True
            sub["status"] = "completed"
        sub["last_check_summary"] = f"匹配 {plan.get('matched_count', 0)} 个文件，规划新增 {len(new_files)} 个，当前进度 {sub['current_episode_number']}。"
        sub.setdefault("check_history", [])
        sub["check_history"].append({
            "time": int(time.time()),
            "state": sub["status"],
            "matched_count": plan.get("matched_count", 0),
            "transfer_count": len(new_files),
            "new_files": new_files,
            "new_episodes": new_episodes,
            "summary": sub["last_check_summary"],
        })
        sub["check_history"] = sub["check_history"][-30:]
        self.save()
        became_invalid = is_invalid and not was_invalid
        return subscription_view(sub), new_files, became_invalid

    def mark_transferred(self, sub_id: str, transfers: list[dict[str, Any]]) -> dict[str, Any] | None:
        sub = next((x for x in self._items if x.get("id") == sub_id), None)
        if not sub:
            return None
        transferred_keys = set(sub.get("transferred_file_keys") or [])
        transferred_names = set(sub.get("transferred_files") or [])
        for item in transfers:
            if item.get("file_key"):
                transferred_keys.add(item["file_key"])
            if item.get("source_name"):
                transferred_names.add(item["source_name"])
        sub["transferred_file_keys"] = sorted(transferred_keys)
        sub["transferred_files"] = sorted(transferred_names)
        sub["updated_at"] = int(time.time())
        self.save()
        return subscription_view(sub)

    def delete(self, sub_id: str) -> bool:
        before = len(self._items)
        self._items = [x for x in self._items if x.get("id") != sub_id]
        changed = len(self._items) != before
        if changed:
            self.save()
        return changed


subscription_store = SubscriptionStore()
