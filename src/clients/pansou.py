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


class RemotePanSouClient:
    """Client for remote PanSou API service.
    
    Uses a deployed PanSou Go service with full plugin ecosystem (91+ plugins).
    This provides much better search results than the inline implementation.
    """

    def __init__(self, base_url: str | None = None):
        self.base_url = (base_url or "https://pansou.lxf87.com.cn").rstrip("/")
        self.session = requests.Session()
        self.session.headers.update({
            "User-Agent": USER_AGENT,
            "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
        })

    def search(self, keyword: str, cloud_types: list[str] | None = None, limit: int = 10):
        cloud_types = cloud_types or ["quark"]
        if "quark" not in cloud_types:
            return []
        
        try:
            # Call remote PanSou API with correct parameters
            api_url = f"{self.base_url}/api/search"
            params = {
                "kw": keyword,           # 正确参数：kw 不是 keyword
                "res": "merge",          # 结果类型：merge 返回 merged_by_type
                "src": "all",            # 数据来源：all = TG频道 + 插件
            }
            resp = self.session.get(api_url, params=params, timeout=15)
            resp.raise_for_status()
            data = resp.json()
            
            if data.get("code") != 0:
                return []
            
            # Extract quark results from merged_by_type
            quark_results = data.get("data", {}).get("merged_by_type", {}).get("quark", [])
            
            # Convert to internal format - no filtering needed, pansou already does it
            results = []
            for item in quark_results:
                results.append({
                    "unique_id": f"pansou:{md5(item['url'].encode()).hexdigest()[:12]}",
                    "note": item.get("note", ""),
                    "url": item.get("url", ""),
                    "password": item.get("password", ""),
                    "source": item.get("source", "pansou"),
                    "datetime": item.get("datetime", datetime.now(timezone.utc).isoformat()),
                    "images": item.get("images", []),
                    "cloud_type": "quark",
                })
            
            return results[:limit]
        except Exception as e:
            # Fallback to empty results on error
            return []

    def search_quark(self, keyword: str, limit: int = 10):
        return self.search(keyword, ["quark"], limit)

    def _dedupe_and_rank(self, items: list[dict[str, Any]], keyword: str) -> list[dict[str, Any]]:
        # Not needed for remote API, but keep for compatibility
        return items


class InlinePanSouClient:
    """Small in-process PanSou-style search aggregator.

    This is a fallback implementation with limited plugins.
    Prefer RemotePanSouClient for better results.
    """

    sources = (
        SearchSource("wanou", 1, 8.0),
        SearchSource("labi", 1, 8.0),
        SearchSource("lou1", 1, 12.0),
        SearchSource("quark4k", 2, 10.0),
        SearchSource("quarksoo", 3, 8.0),
        SearchSource("pansearch", 4, 10.0),
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
        if source.name == "wanou":
            return self._search_wanou(keyword, source)
        if source.name == "labi":
            return self._search_labi(keyword, source)
        if source.name == "lou1":
            return self._search_lou1(keyword, source)
        if source.name == "quarksoo":
            return self._search_quarksoo(keyword, source)
        if source.name == "quark4k":
            return self._search_quark4k(keyword, source)
        if source.name == "pansearch":
            return self._search_pansearch(keyword, source)
        return []

    def _search_wanou(self, keyword: str, source: SearchSource) -> list[dict[str, Any]]:
        api = "https://woog.nxog.eu.org/api.php/provide/vod"
        resp = self.session.get(api, params={"ac": "detail", "wd": keyword}, headers={"Referer": "https://woog.nxog.eu.org/"}, timeout=source.timeout)
        resp.raise_for_status()
        data = resp.json()
        items = []
        for entry in data.get("list") or []:
            title = _clean_text(entry.get("vod_name") or "")
            if not title or not _keyword_match(title, keyword):
                continue
            content = " ".join(str(entry.get(key) or "") for key in ("vod_remarks", "vod_year", "vod_area", "vod_actor", "vod_content"))
            text = " ".join(str(entry.get(key) or "") for key in ("vod_down_url", "vod_play_url", "vod_content"))
            for link in _extract_quark_links(text):
                note = title
                if entry.get("vod_remarks"):
                    note = f"{note} {entry.get('vod_remarks')}"
                items.append(_item(note, link["url"], "plugin:wanou", source.rank, password=link.get("password") or ""))
        return items

    def _search_labi(self, keyword: str, source: SearchSource) -> list[dict[str, Any]]:
        search_url = f"http://xiaocge.fun/index.php/vod/search/wd/{quote_plus(keyword)}.html"
        resp = self.session.get(search_url, headers={"Referer": "http://xiaocge.fun/"}, timeout=source.timeout)
        resp.raise_for_status()
        detail_pattern = re.compile(r'<a[^>]+href=["\']([^"\']*/vod/detail/id/(\d+)\.html[^"\']*)["\'][^>]*>(.*?)</a>', re.I | re.S)
        detail_urls: list[tuple[str, str]] = []
        seen_ids: set[str] = set()
        for href, item_id, title_html in detail_pattern.findall(resp.text):
            title = _clean_text(title_html)
            if not title or item_id in seen_ids or not _keyword_match(title, keyword):
                continue
            seen_ids.add(item_id)
            detail_urls.append((title, _absolute_url("http://xiaocge.fun", href)))
            if len(detail_urls) >= 12:
                break
        items = []
        for title, detail_url in detail_urls:
            try:
                detail = self.session.get(detail_url, headers={"Referer": "http://xiaocge.fun/"}, timeout=source.timeout)
                detail.raise_for_status()
            except Exception:
                continue
            for link in _extract_quark_links(detail.text):
                items.append(_item(title, link["url"], "plugin:labi", source.rank, password=link.get("password") or ""))
        return items

    def _search_lou1(self, keyword: str, source: SearchSource) -> list[dict[str, Any]]:
        search_url = f"https://www.1lou.me/search-{quote_plus(keyword)}.htm"
        resp = self.session.get(search_url, headers={"Referer": "https://www.1lou.me/"}, timeout=source.timeout)
        resp.raise_for_status()
        block_pattern = re.compile(r'<li[^>]+class=["\'][^"\']*media thread[^"\']*["\'][\s\S]*?</li>', re.I)
        link_pattern = re.compile(r'<div[^>]+class=["\'][^"\']*subject[^"\']*["\'][\s\S]*?<a[^>]+href=["\']([^"\']*thread-\d+\.htm)["\'][^>]*>([\s\S]*?)</a>', re.I)
        details: list[tuple[str, str]] = []
        seen = set()
        for block in block_pattern.findall(resp.text):
            match = link_pattern.search(block)
            if not match:
                continue
            href, title_html = match.groups()
            title = _clean_text(title_html)
            if not title or href in seen or not _keyword_match(title, keyword):
                continue
            seen.add(href)
            details.append((title, _absolute_url("https://www.1lou.me", href)))
            if len(details) >= 12:
                break
        items = []
        for title, detail_url in details:
            try:
                detail = self.session.get(detail_url, headers={"Referer": "https://www.1lou.me/"}, timeout=source.timeout)
                detail.raise_for_status()
            except Exception:
                continue
            for link in _extract_quark_links(detail.text):
                items.append(_item(title, link["url"], "plugin:lou1", source.rank, password=link.get("password") or ""))
        return items

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


class HybridPanSouClient:
    """Hybrid client that combines remote API and inline sources.
    
    Uses both remote PanSou API and inline sources, then merges and deduplicates.
    This provides better coverage than either approach alone.
    """

    def __init__(self, base_url: str | None = None):
        self.remote = RemotePanSouClient(base_url)
        self.inline = InlinePanSouClient()
        
    def search(self, keyword: str, cloud_types: list[str] | None = None, limit: int = 10):
        # Get results from both sources
        remote_results = self.remote.search(keyword, cloud_types, limit * 2)
        inline_results = self.inline.search(keyword, cloud_types, limit * 2)
        
        # Merge and deduplicate by URL
        seen_urls = set()
        merged = []
        
        # Prioritize inline results (better quality)
        for item in inline_results:
            url = _normalize_quark_url(item.get("url", ""))
            if url and url not in seen_urls:
                seen_urls.add(url)
                merged.append(item)
        
        # Add remote results that aren't duplicates
        for item in remote_results:
            url = _normalize_quark_url(item.get("url", ""))
            if url and url not in seen_urls:
                seen_urls.add(url)
                merged.append(item)
        
        return merged[:limit]
    
    def search_quark(self, keyword: str, limit: int = 10):
        return self.search(keyword, ["quark"], limit)


PanSouClient = HybridPanSouClient


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


def _absolute_url(base: str, href: str) -> str:
    href = html.unescape(str(href or "")).strip()
    if href.startswith("http://") or href.startswith("https://"):
        return href
    if not href.startswith("/"):
        href = "/" + href
    return base.rstrip("/") + href


def _extract_quark_links(text: str) -> list[dict[str, str]]:
    source_text = html.unescape(str(text or ""))
    results = []
    seen = set()
    for match in QUARK_URL_RE.finditer(source_text):
        url = _normalize_quark_url(match.group(0))
        if not url or url in seen:
            continue
        tail = source_text[match.end():match.end() + 120]
        pwd = PASSWORD_RE.search(tail)
        results.append({"url": url, "password": pwd.group(1) if pwd else ""})
        seen.add(url)
    return results


def _walk_json(value: Any):
    yield value
    if isinstance(value, dict):
        for child in value.values():
            yield from _walk_json(child)
    elif isinstance(value, list):
        for child in value:
            yield from _walk_json(child)
