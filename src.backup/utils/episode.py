from __future__ import annotations

import re
from pathlib import Path
from typing import Any

VIDEO_EXTS = {".mkv", ".mp4", ".avi", ".ts", ".mov", ".wmv", ".flv", ".m4v", ".rmvb", ".webm"}

EPISODE_PATTERNS = [
    re.compile(r"S(?P<season>\d{1,2})E(?P<episode>\d{1,4})", re.I),
    re.compile(r"EP?\s*(?P<episode>\d{1,4})", re.I),
    re.compile(r"第\s*(?P<episode>\d{1,4})\s*[集话期]"),
    re.compile(r"[\[【第_\-\s\.](?P<episode>\d{1,4})[\]】_\-\s\.](?=.*\.(mkv|mp4|avi|ts|mov|wmv|flv|m4v|rmvb|webm)$)", re.I),
    re.compile(r"(?P<episode>\d{1,4})(?=\.(mkv|mp4|avi|ts|mov|wmv|flv|m4v|rmvb|webm)$)", re.I),
]


def is_video_name(name: str) -> bool:
    return Path(name).suffix.lower() in VIDEO_EXTS


def detect_episode(name: str) -> dict[str, Any]:
    for pat in EPISODE_PATTERNS:
        m = pat.search(name)
        if m:
            episode = int(m.group("episode"))
            season = int(m.groupdict().get("season") or 0) or None
            return {"episode": episode, "season": season}
    return {"episode": None, "season": None}


def split_words(value: str | list[str] | None) -> list[str]:
    if not value:
        return []
    if isinstance(value, list):
        return [str(x).strip() for x in value if str(x).strip()]
    return [x.strip() for x in re.split(r"[,，\n]", value) if x.strip()]


def match_file(name: str, include_keywords=None, exclude_keywords=None, regex: str = "") -> bool:
    include = split_words(include_keywords)
    exclude = split_words(exclude_keywords)
    lower = name.lower()
    if include and not any(word.lower() in lower for word in include):
        return False
    if exclude and any(word.lower() in lower for word in exclude):
        return False
    if regex:
        try:
            if not re.search(regex, name):
                return False
        except re.error:
            return False
    return True


def normalize_probe_files(files: list[dict[str, Any]], include_keywords=None, exclude_keywords=None, regex: str = "") -> list[dict[str, Any]]:
    out = []
    for f in files:
        name = f.get("name") or ""
        if f.get("is_dir"):
            continue
        if not match_file(name, include_keywords, exclude_keywords, regex):
            continue
        ep = detect_episode(name)
        item = dict(f)
        item["is_video"] = is_video_name(name)
        item["episode"] = ep["episode"]
        item["season"] = ep["season"]
        item["file_key"] = f"ep:{ep['episode']}:{name}" if ep["episode"] is not None else f"name:{name}"
        out.append(item)
    return out


def summarize_episodes(files: list[dict[str, Any]]) -> dict[str, Any]:
    episodes = sorted({f.get("episode") for f in files if f.get("episode") is not None})
    current = max(episodes) if episodes else 0
    return {"episodes": episodes, "current_episode_number": current, "episode_count": len(episodes)}
