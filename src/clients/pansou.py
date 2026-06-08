from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from datetime import datetime, timezone
from hashlib import md5
import html
import re
from typing import Any
from urllib.parse import quote_plus

import requests

USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36"
QUARK_URL_RE = re.compile(r"https?://pan\.(?:quark|qoark)\.cn/s/[A-Za-z0-9_-]{8,}", re.I)
PASSWORD_RE = re.compile(r"(?:提取码|密码|访问码|pwd)[:：\s]*([A-Za-z0-9]{3,8})", re.I)
TAG_RE = re.compile(r"<[^>]+>")


@dataclass(frozen=True)
class SearchSource:
    name: str
    rank: int
    timeout: float = 8.0


class InlinePanSouClient:
    """Small in-process PanSou-style search aggregator.

    PanSou itself is a Go service with a large plugin ecosystem. This client keeps
    the API shape this project needs, but executes selected public-source plugins
    directly inside the FastAPI process so no external PanSou service is required.
    """

    sources = (
        SearchSource("quarksoo", 1, 8.0),
        SearchSource("quark4k", 2, 10.0),
        SearchSource("pansearch", 3, 10.0),
    )

    def __init__(self, base_url: str | None = None):
        self.base_url = (base_url or "inline").rstrip("/")
        self.session = requests.Session()
        self.session.headers.update({
            "User-Agent": USER_AGENT,
            "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
        })

    def search(self, keyword: str, cloud_types: list[str] | None = None, limit: int = 10):
        cloud_types = cloud_types or ["quark"]
        if "quark" not in cloud_types:
            return []
        results: list[dict[str, Any]] = []
        with ThreadPoolExecutor(max_workers=len(self.sources)) as executor:
            futures = {executor.submit(self._search_source, source, keyword): source for source in self.sources}
            for future in as_completed(futures):
                try:
                    results.extend(future.result())
                except Exception:
                    continue
        ranked = self._dedupe_and_rank(results, keyword)
        return ranked[:limit]

    def search_quark(self, keyword: str, limit: int = 10):
        return self.search(keyword, ["quark"], limit)

    def _search_source(self, source: SearchSource, keyword: str) -> list[dict[str, Any]]:
        if source.name == "quarksoo":
            return self._search_quarksoo(keyword, source)
        if source.name == "quark4k":
            return self._search_quark4k(keyword, source)
        if source.name == "pansearch":
            return self._search_pansearch(keyword, source)
        return []

    def _search_quarksoo(self, keyword: str, source: SearchSource) -> list[dict[str, Any]]:
        url = f"https://quarksoo.cc/search.php?q={quote_plus(keyword)}"
        resp = self.session.get(url, headers={"Referer": "https://quarksoo.cc/"}, timeout=source.timeout)
        resp.raise_for_status()
        text = resp.text
        pattern = re.compile(r"<tr>\s*<td>(.*?)</td>\s*<td>\s*<a[^>]*href=[\"']([^\"']+)[\"']", re.I | re.S)
        items = []
        for title_html, url in pattern.findall(text):
            title = _clean_text(title_html)
            if not title or not _keyword_match(title, keyword):
                continue
            if not QUARK_URL_RE.search(url):
                continue
            items.append(_item(title, url, "plugin:quarksoo", source.rank))
        return items

    def _search_quark4k(self, keyword: str, source: SearchSource) -> list[dict[str, Any]]:
        api = "https://quark4k.com/api/discussions"
        params = {
            "include": "user,lastPostedUser,mostRelevantPost,mostRelevantPost.user,tags,tags.parent,firstPost",
            "filter[q]": keyword,
            "sort": "",
            "page[offset]": 0,
            "page[limit]": 30,
        }
        resp = self.session.get(api, params=params, headers={"Referer": "https://quark4k.com/"}, timeout=source.timeout)
        resp.raise_for_status()
        data = resp.json()
        included = {str(item.get("id")): item for item in data.get("included") or []}
        items = []
        for discussion in data.get("data") or []:
            attrs = discussion.get("attributes") or {}
            title = attrs.get("title") or attrs.get("slug") or "夸克资源"
            content = ""
            rel = (((discussion.get("relationships") or {}).get("firstPost") or {}).get("data") or {})
            post = included.get(str(rel.get("id"))) if rel else None
            if post:
                post_attrs = post.get("attributes") or {}
                content = post_attrs.get("contentHtml") or post_attrs.get("content") or ""
            haystack = f"{title}\n{content}"
            if not _keyword_match(haystack, keyword):
                continue
            for link in _extract_quark_links(haystack):
                items.append(_item(_clean_text(title), link["url"], "plugin:quark4k", source.rank, password=link.get("password") or ""))
        return items

    def _search_pansearch(self, keyword: str, source: SearchSource) -> list[dict[str, Any]]:
        build_id = self._pansearch_build_id(source.timeout)
        if not build_id:
            return []
        api = f"https://www.pansearch.me/_next/data/{build_id}/search.json"
        resp = self.session.get(api, params={"keyword": keyword, "pan": "quark"}, headers={"Referer": "https://www.pansearch.me/search"}, timeout=source.timeout)
        resp.raise_for_status()
        data = resp.json()
        candidates = _walk_json(data)
        items = []
        for value in candidates:
            if not isinstance(value, dict):
                continue
            text = " ".join(str(value.get(k) or "") for k in ("title", "name", "content", "desc", "description"))
            if not _keyword_match(text, keyword):
                continue
            for link in _extract_quark_links(str(value)):
                title = _clean_text(text) or keyword
                items.append(_item(title[:160], link["url"], "plugin:pansearch", source.rank, password=link.get("password") or ""))
        return items

    def _pansearch_build_id(self, timeout: float) -> str | None:
        resp = self.session.get("https://www.pansearch.me/search", timeout=timeout)
        resp.raise_for_status()
        match = re.search(r'"buildId":"([^"]+)"', resp.text)
        return match.group(1) if match else None

    def _dedupe_and_rank(self, items: list[dict[str, Any]], keyword: str) -> list[dict[str, Any]]:
        seen: dict[str, dict[str, Any]] = {}
        for item in items:
            url = _normalize_quark_url(item.get("url") or "")
            if not url:
                continue
            item["url"] = url
            prev = seen.get(url)
            if not prev or item.get("_rank", 99) < prev.get("_rank", 99):
                seen[url] = item
        words = [w.lower() for w in re.split(r"\s+", keyword.strip()) if w]
        ranked = list(seen.values())
        ranked.sort(key=lambda item: (
            item.get("_rank", 99),
            -sum(1 for word in words if word in (item.get("note") or "").lower()),
            item.get("note") or "",
        ))
        for item in ranked:
            item.pop("_rank", None)
        return ranked


PanSouClient = InlinePanSouClient


def _item(title: str, url: str, source: str, rank: int, password: str = "") -> dict[str, Any]:
    url = _normalize_quark_url(url)
    digest = md5(f"{title}|{url}".encode("utf-8")).hexdigest()[:12]
    return {
        "unique_id": f"{source}:{digest}",
        "note": title,
        "url": url,
        "password": password,
        "source": source,
        "datetime": datetime.now(timezone.utc).isoformat(),
        "images": [],
        "cloud_type": "quark",
        "_rank": rank,
    }


def _clean_text(value: str) -> str:
    value = html.unescape(str(value or ""))
    value = TAG_RE.sub(" ", value)
    return re.sub(r"\s+", " ", value).strip()


def _keyword_match(text: str, keyword: str) -> bool:
    text = _clean_text(text).lower()
    words = [w for w in re.split(r"\s+", keyword.lower().strip()) if w]
    return all(word in text for word in words) if words else True


def _normalize_quark_url(url: str) -> str:
    match = QUARK_URL_RE.search(str(url or ""))
    if not match:
        return ""
    return match.group(0).replace("pan.qoark.cn", "pan.quark.cn")


def _extract_quark_links(text: str) -> list[dict[str, str]]:
    results = []
    for match in QUARK_URL_RE.finditer(str(text or "")):
        tail = str(text or "")[match.end():match.end() + 80]
        pwd = PASSWORD_RE.search(tail)
        results.append({"url": match.group(0), "password": pwd.group(1) if pwd else ""})
    return results


def _walk_json(value: Any):
    yield value
    if isinstance(value, dict):
        for child in value.values():
            yield from _walk_json(child)
    elif isinstance(value, list):
        for child in value:
            yield from _walk_json(child)
