from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import Any


@dataclass
class SearchSession:
    keyword: str
    results: list[dict[str, Any]]
    created_at: float = field(default_factory=time.time)


class MemorySessionStore:
    def __init__(self, ttl_seconds: int = 3600):
        self.ttl_seconds = ttl_seconds
        self._sessions: dict[str, SearchSession] = {}

    def set(self, key: str, keyword: str, results: list[dict[str, Any]]):
        self._sessions[key] = SearchSession(keyword=keyword, results=results)

    def get(self, key: str) -> SearchSession | None:
        sess = self._sessions.get(key)
        if not sess:
            return None
        if time.time() - sess.created_at > self.ttl_seconds:
            self._sessions.pop(key, None)
            return None
        return sess
