from __future__ import annotations

from ..clients.quark import QuarkShareProbe


class InlineLinkChecker:
    """Project-local replacement for PanSou /api/check/links."""

    def __init__(self, base_url: str | None = None):
        self.base_url = base_url or "inline"

    def check_quark(self, items: list[dict], timeout: int = 30) -> list[dict]:
        probe = QuarkShareProbe()
        results = []
        for item in items:
            url = item.get("url") or ""
            if not url:
                continue
            info = probe.probe(url, item.get("password") or "")
            state = "good" if info.ok else ("bad" if info.state in {"not_found", "expired", "deleted"} else info.state or "unknown")
            results.append({
                "disk_type": "quark",
                "url": url,
                "normalized_url": url,
                "state": state,
                "summary": info.message,
            })
        return results


PanSouLinkChecker = InlineLinkChecker
