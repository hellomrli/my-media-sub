from __future__ import annotations

import re
from pathlib import Path
from typing import Any

from ..utils.episode import detect_episode, split_words, summarize_episodes

DEFAULT_EXCLUDES = ["预告", "花絮", "解说", "彩蛋", "trailer", "preview"]


def default_transfer_rules() -> dict[str, Any]:
    """Default rule model inspired by quark-auto-save task rules.

    This module is intentionally pure: no persistence, no network calls, no API/web
    concerns. It only turns subscription rules + observed files + state into a
    transfer plan that a future Quark executor can consume.
    """
    return {
        "target_dir": "",
        "auto_create_target_dir": True,
        "skip_existing_transferred": True,
        "include_keywords": [],
        "exclude_keywords": list(DEFAULT_EXCLUDES),
        "match_regex": "",
        "ignore_extensions": False,
        "rename_regex": "",
        "rename_replacement": "",
        "rename_template": "",
        "only_latest": False,
        "notify_on_update": True,
        "notify_on_invalid": True,
        "check_interval_minutes": 60,
        "check_weekdays": [],
        "finish_after_episode": None,
    }


def normalize_rules(rules: dict[str, Any] | None) -> dict[str, Any]:
    normalized = default_transfer_rules()
    if isinstance(rules, dict):
        for key in normalized:
            if key in rules:
                normalized[key] = rules[key]
    normalized["include_keywords"] = split_words(normalized.get("include_keywords"))
    normalized["exclude_keywords"] = split_words(normalized.get("exclude_keywords"))
    normalized["target_dir"] = str(normalized.get("target_dir") or "").strip()
    normalized["match_regex"] = str(normalized.get("match_regex") or "").strip()
    normalized["rename_regex"] = str(normalized.get("rename_regex") or "").strip()
    normalized["rename_replacement"] = str(normalized.get("rename_replacement") or "")
    normalized["rename_template"] = str(normalized.get("rename_template") or "").strip()
    normalized["auto_create_target_dir"] = bool(normalized.get("auto_create_target_dir"))
    normalized["skip_existing_transferred"] = bool(normalized.get("skip_existing_transferred"))
    normalized["ignore_extensions"] = bool(normalized.get("ignore_extensions"))
    normalized["only_latest"] = bool(normalized.get("only_latest"))
    return normalized


def display_name(name: str, ignore_extensions: bool = False) -> str:
    if not ignore_extensions:
        return name
    suffix = Path(name).suffix
    return name[: -len(suffix)] if suffix else name


def state_key(name: str, episode: int | None = None, ignore_extensions: bool = False) -> str:
    comparable = display_name(name, ignore_extensions).lower()
    if episode is not None:
        return f"ep:{episode}:{comparable}"
    return f"name:{comparable}"


def _has_words(name: str, words: list[str]) -> bool:
    lower = name.lower()
    return any(word.lower() in lower for word in words)


def _format_episode(episode: int | None) -> str:
    if episode is None:
        return ""
    return f"{episode:02d}" if episode < 100 else str(episode)


def apply_rename(name: str, rules: dict[str, Any], subscription: dict[str, Any] | None = None, episode: int | None = None) -> tuple[str, str | None]:
    """Return target filename and optional error message."""
    ignore_ext = bool(rules.get("ignore_extensions"))
    suffix = Path(name).suffix
    base = display_name(name, ignore_ext)
    rename_input = base if ignore_ext else name
    target = rename_input

    regex = rules.get("rename_regex") or ""
    replacement = rules.get("rename_replacement") or ""
    if regex:
        try:
            target = re.sub(regex, replacement, target)
        except re.error as exc:
            return name, f"rename_regex 无效：{exc}"

    template = rules.get("rename_template") or ""
    if template:
        context = {
            "title": (subscription or {}).get("title") or "",
            "season": (subscription or {}).get("season") or 1,
            "episode": _format_episode(episode),
            "episode_number": episode or "",
            "original": display_name(name, True),
            "name": display_name(target, True),
            "ext": suffix.lstrip("."),
        }
        try:
            # quark-auto-save uses magic variables; this project starts small with
            # Python templates plus QAS-like bare {} episode placeholder.
            if "{}" in template:
                target = template.replace("{}", context["episode"])
            else:
                target = template.format(**context)
        except Exception as exc:
            return name, f"rename_template 无效：{exc}"

    known_media_suffixes = {".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv", ".ts", ".m2ts", ".webm", ".srt", ".ass", ".ssa"}
    if suffix and not any(target.lower().endswith(ext) for ext in known_media_suffixes):
        target = f"{target}{suffix}"
    return target or name, None


def build_transfer_plan(
    subscription: dict[str, Any],
    probe_files: list[dict[str, Any]] | None = None,
    *,
    transferred_keys: list[str] | set[str] | None = None,
    target_existing_files: list[str] | set[str] | None = None,
    target_dir_exists: bool | None = None,
) -> dict[str, Any]:
    rules = normalize_rules(subscription.get("rules") if subscription else None)
    files = list(probe_files if probe_files is not None else ((subscription or {}).get("last_probe") or {}).get("files") or [])
    transferred = set(transferred_keys if transferred_keys is not None else (subscription.get("transferred_file_keys") or []))
    existing = list(target_existing_files if target_existing_files is not None else (subscription.get("target_existing_files") or []))
    existing_compare = {display_name(x, rules["ignore_extensions"]).lower() for x in existing}
    target_dir = rules.get("target_dir") or f"/{subscription.get('title') or '未命名订阅'}"

    items: list[dict[str, Any]] = []
    matched_for_summary: list[dict[str, Any]] = []
    compile_error = None
    match_re = None
    if rules["match_regex"]:
        try:
            match_re = re.compile(rules["match_regex"])
        except re.error as exc:
            compile_error = str(exc)

    for raw in files:
        name = raw.get("name") or raw.get("file_name") or ""
        ep = detect_episode(name)
        episode = ep.get("episode")
        key = state_key(name, episode, rules["ignore_extensions"])
        item = {
            "source_name": name,
            "source_fid": raw.get("fid") or raw.get("file_id") or raw.get("id"),
            "episode": episode,
            "season": ep.get("season"),
            "file_key": key,
            "target_dir": target_dir,
            "target_name": name,
            "action": "skip",
            "skip_reason": "",
        }
        comparable = display_name(name, rules["ignore_extensions"])
        if raw.get("is_dir") or raw.get("dir"):
            item["skip_reason"] = "目录暂不规划转存"
        elif not name:
            item["skip_reason"] = "文件名为空"
        elif rules["include_keywords"] and not _has_words(comparable, rules["include_keywords"]):
            item["skip_reason"] = "不含包含关键词"
        elif rules["exclude_keywords"] and _has_words(comparable, rules["exclude_keywords"]):
            item["skip_reason"] = "命中排除关键词"
        elif compile_error:
            item["skip_reason"] = f"match_regex 无效：{compile_error}"
        elif match_re and not match_re.search(comparable):
            item["skip_reason"] = "未命中匹配正则"
        elif rules["skip_existing_transferred"] and key in transferred:
            item["skip_reason"] = "已转存记录中存在"
        else:
            target_name, rename_error = apply_rename(name, rules, subscription, episode)
            item["target_name"] = target_name
            target_compare = display_name(target_name, rules["ignore_extensions"]).lower()
            if rename_error:
                item["skip_reason"] = rename_error
            elif target_compare in existing_compare:
                item["skip_reason"] = "目标目录已有同名文件"
            elif target_dir_exists is False and not rules["auto_create_target_dir"]:
                item["skip_reason"] = "目标目录不存在且未开启自动新建"
            else:
                item["action"] = "transfer"
                item["skip_reason"] = ""
                matched_for_summary.append({"name": name, "episode": episode, "file_key": key})
        items.append(item)

    if rules["only_latest"]:
        transfer_items = [x for x in items if x["action"] == "transfer"]
        episodes = [x.get("episode") or 0 for x in transfer_items]
        latest = max(episodes) if episodes else None
        if latest is not None:
            matched_for_summary = []
            for item in items:
                if item["action"] == "transfer" and (item.get("episode") or 0) != latest:
                    item["action"] = "skip"
                    item["skip_reason"] = "only_latest 仅处理最新一集"
                if item["action"] == "transfer":
                    matched_for_summary.append({"name": item["source_name"], "episode": item.get("episode"), "file_key": item["file_key"]})

    transfers = [x for x in items if x["action"] == "transfer"]
    skipped = [x for x in items if x["action"] == "skip"]
    normalized_matched = [
        {"name": x["source_name"], "episode": x.get("episode"), "file_key": x["file_key"]}
        for x in items
        if not x.get("skip_reason", "").startswith("match_regex 无效") and x.get("skip_reason") not in {"目录暂不规划转存", "文件名为空", "不含包含关键词", "命中排除关键词", "未命中匹配正则"}
    ]
    episode_summary = summarize_episodes(normalized_matched)
    return {
        "target_dir": target_dir,
        "target_dir_exists": target_dir_exists,
        "auto_create_target_dir": rules["auto_create_target_dir"],
        "items": items,
        "transfers": transfers,
        "skipped": skipped,
        "transfer_count": len(transfers),
        "skip_count": len(skipped),
        "matched_count": len(normalized_matched),
        "episodes": episode_summary["episodes"],
        "current_episode_number": episode_summary["current_episode_number"],
        "summary": f"规划转存 {len(transfers)} 个，跳过 {len(skipped)} 个，目标目录：{target_dir}",
    }


def summarize_rules(rules: dict[str, Any] | None) -> str:
    rules = normalize_rules(rules)
    parts = []
    if rules["target_dir"]:
        parts.append(f"目录 {rules['target_dir']}")
    if rules["match_regex"]:
        parts.append(f"正则 {rules['match_regex']}")
    if rules["include_keywords"]:
        parts.append("包含 " + "/".join(rules["include_keywords"]))
    if rules["exclude_keywords"]:
        parts.append("排除 " + "/".join(rules["exclude_keywords"][:4]))
    if rules["rename_template"]:
        parts.append(f"模板 {rules['rename_template']}")
    if rules["rename_regex"]:
        parts.append(f"替换 {rules['rename_regex']}→{rules['rename_replacement']}")
    if rules["only_latest"]:
        parts.append("仅最新")
    if rules["skip_existing_transferred"]:
        parts.append("跳过已转存")
    return "；".join(parts) or "默认规则"
