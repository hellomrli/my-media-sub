from __future__ import annotations

import asyncio
from dataclasses import dataclass
from datetime import datetime, timezone
from hashlib import md5
import html
import re
from typing import Any
from urllib.parse import quote_plus

import httpx
from tenacity import retry, stop_after_attempt, wait_exponential


USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36"
QUARK_URL_RE = re.compile(r"https?://pan\.(?:quark|qoark)\.cn/s/[A-Za-z0-9_-]{8,}", re.I)
PASSWORD_RE = re.compile(r"(?:提取码|密码|访问码|pwd)[:：\s]*([A-Za-z0-9]{3,8})", re.I)
TAG_RE = re.compile(r"<[^>]+>")


@dataclass(frozen=True)
class SearchSource:
    name: str
    rank: int
    timeout: float = 8.0


def _clean_text(text: str) -> str:
    text = TAG_RE.sub("", text)
    text = html.unescape(text)
    return " ".join(text.split()).strip()


def _keyword_match(text: str, keyword: str) -> bool:
    text_lower = text.lower()
    keyword_lower = keyword.lower()
    return keyword_lower in text_lower


def _extract_quark_links(text: str) -> list[tuple[str, str]]:
    links = QUARK_URL_RE.findall(text)
    result = []
    for link in links:
        pwd_match = PASSWORD_RE.search(text[text.find(link):text.find(link) + 200])
        pwd = pwd_match.group(1) if pwd_match else ""
        result.append((link, pwd))
    return result


class InlinePanSouClientAsync:
    """Async version of InlinePanSouClient with concurrent source fetching."""

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
        self.headers = {
            "User-Agent": USER_AGENT,
            "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
        }

    async def search(self, keyword: str, cloud_types: list[str] | None = None, limit: int = 10):
        cloud_types = cloud_types or ["quark"]
        if "quark" not in cloud_types:
            return []
        
        async with httpx.AsyncClient(headers=self.headers) as client:
            tasks = [self._search_source(client, source, keyword) for source in self.sources]
            results_lists = await asyncio.gather(*tasks, return_exceptions=True)
            
        results: list[dict[str, Any]] = []
        for result in results_lists:
            if isinstance(result, Exception):
                continue
            if isinstance(result, list):
                results.extend(result)
        
        ranked = self._dedupe_and_rank(results, keyword)
        return ranked[:limit]

    async def search_quark(self, keyword: str, limit: int = 10):
        return await self.search(keyword, ["quark"], limit)

    async def _search_source(self, client: httpx.AsyncClient, source: SearchSource, keyword: str) -> list[dict[str, Any]]:
        try:
            if source.name == "wanou":
                return await self._search_wanou(client, keyword, source)
            if source.name == "labi":
                return await self._search_labi(client, keyword, source)
            if source.name == "lou1":
                return await self._search_lou1(client, keyword, source)
            if source.name == "quarksoo":
                return await self._search_quarksoo(client, keyword, source)
            if source.name == "quark4k":
                return await self._search_quark4k(client, keyword, source)
            if source.name == "pansearch":
                return await self._search_pansearch(client, keyword, source)
        except Exception:
            pass
        return []

    @retry(stop=stop_after_attempt(2), wait=wait_exponential(multiplier=1, min=1, max=4))
    async def _search_wanou(self, client: httpx.AsyncClient, keyword: str, source: SearchSource) -> list[dict[str, Any]]:
        api = "https://woog.nxog.eu.org/api.php/provide/vod"
        resp = await client.get(api, params={"ac": "detail", "wd": keyword}, headers={"Referer": "https://woog.nxog.eu.org/"}, timeout=source.timeout)
        resp.raise_for_status()
        data = resp.json()
        items = []
        for entry in data.get("list") or []:
            title = _clean_text(entry.get("vod_name") or "")
            if not title or not _keyword_match(title, keyword):
                continue
            text = " ".join(str(entry.get(key) or "") for key in ("vod_down_url", "vod_play_url", "vod_content"))
            for link, pwd in _extract_quark_links(text):
                note = title
                if entry.get("vod_remarks"):
                    note = f"{note} {entry.get('vod_remarks')}"
                items.append(self._make_item(link, pwd, note, source.name, entry.get("vod_time") or "", []))
        return items

    @retry(stop=stop_after_attempt(2), wait=wait_exponential(multiplier=1, min=1, max=4))
    async def _search_labi(self, client: httpx.AsyncClient, keyword: str, source: SearchSource) -> list[dict[str, Any]]:
        api = "https://www.labi.tw/api.php/provide/vod"
        resp = await client.get(api, params={"ac": "detail", "wd": keyword}, timeout=source.timeout)
        resp.raise_for_status()
        data = resp.json()
        items = []
        for entry in data.get("list") or []:
            title = _clean_text(entry.get("vod_name") or "")
            if not title or not _keyword_match(title, keyword):
                continue
            text = " ".join(str(entry.get(key) or "") for key in ("vod_play_url", "vod_down_url", "vod_content"))
            for link, pwd in _extract_quark_links(text):
                note = title
                if entry.get("vod_remarks"):
                    note = f"{note} {entry.get('vod_remarks')}"
                items.append(self._make_item(link, pwd, note, source.name, entry.get("vod_time") or "", []))
        return items

    @retry(stop=stop_after_attempt(2), wait=wait_exponential(multiplier=1, min=1, max=4))
    async def _search_lou1(self, client: httpx.AsyncClient, keyword: str, source: SearchSource) -> list[dict[str, Any]]:
        api = "https://www.louyingw1.com/wp-json/wp/v2/posts"
        resp = await client.get(api, params={"search": keyword, "per_page": 20}, timeout=source.timeout)
        resp.raise_for_status()
        posts = resp.json()
        items = []
        for post in posts:
            title = _clean_text(post.get("title", {}).get("rendered") or "")
            if not title or not _keyword_match(title, keyword):
                continue
            content = _clean_text(post.get("content", {}).get("rendered") or "")
            for link, pwd in _extract_quark_links(content):
                date_str = post.get("date") or ""
                items.append(self._make_item(link, pwd, title, source.name, date_str, []))
        return items

    @retry(stop=stop_after_attempt(2), wait=wait_exponential(multiplier=1, min=1, max=4))
    async def _search_quarksoo(self, client: httpx.AsyncClient, keyword: str, source: SearchSource) -> list[dict[str, Any]]:
        api = "https://quark-so.qianfan.link/search"
        resp = await client.post(api, json={"keyword": keyword, "page": 0, "size": 20}, timeout=source.timeout)
        resp.raise_for_status()
        data = resp.json()
        items = []
        for entry in data.get("data", {}).get("list") or []:
            title = _clean_text(entry.get("title") or "")
            if not title:
                continue
            link = entry.get("url") or ""
            pwd = entry.get("pwd") or ""
            items.append(self._make_item(link, pwd, title, source.name, "", []))
        return items

    @retry(stop=stop_after_attempt(2), wait=wait_exponential(multiplier=1, min=1, max=4))
    async def _search_quark4k(self, client: httpx.AsyncClient, keyword: str, source: SearchSource) -> list[dict[str, Any]]:
        api = "https://quark4k.qianfan.app/api/search"
        resp = await client.post(api, json={"keyword": keyword, "page": 1, "size": 20}, timeout=source.timeout)
        resp.raise_for_status()
        data = resp.json()
        items = []
        for entry in data.get("data", {}).get("list") or []:
            title = _clean_text(entry.get("title") or "")
            if not title:
                continue
            link = entry.get("url") or ""
            pwd = entry.get("pwd") or ""
            items.append(self._make_item(link, pwd, title, source.name, "", []))
        return items

    @retry(stop=stop_after_attempt(2), wait=wait_exponential(multiplier=1, min=1, max=4))
    async def _search_pansearch(self, client: httpx.AsyncClient, keyword: str, source: SearchSource) -> list[dict[str, Any]]:
        api = "https://api.pansearch.me/api/search"
        keyword_enc = quote_plus(keyword)
        resp = await client.get(f"{api}?keyword={keyword_enc}&page=1&size=20", timeout=source.timeout)
        resp.raise_for_status()
        data = resp.json()
        items = []
        for entry in data.get("data", {}).get("list") or []:
            title = _clean_text(entry.get("title") or "")
            if not title:
                continue
            link = entry.get("url") or ""
            pwd = entry.get("pwd") or ""
            items.append(self._make_item(link, pwd, title, source.name, "", []))
        return items

    @staticmethod
    def _make_item(url: str, password: str, note: str, source: str, datetime_str: str, images: list[str]) -> dict[str, Any]:
        return {
            "url": url,
            "password": password or "",
            "note": note,
            "source": source,
            "datetime": datetime_str,
            "images": images,
            "cloud_type": "quark",
        }

    def _dedupe_and_rank(self, results: list[dict[str, Any]], keyword: str) -> list[dict[str, Any]]:
        seen: dict[str, dict[str, Any]] = {}
        for item in results:
            url = item.get("url", "")
            if not url:
                continue
            key = md5(url.lower().encode()).hexdigest()
            if key in seen:
                if seen[key].get("source_rank", 999) > self._source_rank(item.get("source", "")):
                    seen[key] = item
            else:
                seen[key] = item
        
        for item in seen.values():
            item["source_rank"] = self._source_rank(item.get("source", ""))
            item["title_match_score"] = self._title_score(item.get("note", ""), keyword)
        
        ranked = sorted(seen.values(), key=lambda x: (x["source_rank"], -x["title_match_score"]))
        return ranked

    def _source_rank(self, source: str) -> int:
        for s in self.sources:
            if s.name == source:
                return s.rank
        return 999

    @staticmethod
    def _title_score(title: str, keyword: str) -> int:
        title_lower = title.lower()
        keyword_lower = keyword.lower()
        if title_lower == keyword_lower:
            return 100
        if title_lower.startswith(keyword_lower):
            return 80
        if keyword_lower in title_lower:
            return 50
        return 0


class PanSouClient:
    """Legacy sync wrapper - kept for backward compatibility."""
    pass
