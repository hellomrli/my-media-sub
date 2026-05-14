from __future__ import annotations

import re
from typing import Any

from fastapi import Depends, FastAPI, HTTPException
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel, Field

from .aria2_client import Aria2Client
from .auth import require_auth
from .link_checker import PanSouLinkChecker
from .pansou_client import PanSouClient
from .quark_probe import QuarkShareProbe
from .session_store import MemorySessionStore
from .settings_store import settings_store

app = FastAPI(title="my-media-sub", version="0.3.0")
app.mount("/static", StaticFiles(directory="static"), name="static")
sessions = MemorySessionStore()


class SearchRequest(BaseModel):
    keyword: str = Field(..., min_length=1)
    chat_id: str = "default"
    limit: int = Field(default=8, ge=1, le=20)
    cloud_types: list[str] | None = None
    check_links: bool | None = None
    probe_files: bool | None = None
    filter_bad_links: bool | None = None


class SelectRequest(BaseModel):
    chat_id: str = "default"
    index: int = Field(..., ge=1)


class DownloadRequest(BaseModel):
    chat_id: str = "default"
    index: int | None = Field(default=None, ge=1)
    url: str | None = None
    dir: str | None = None


class WechatMessageRequest(BaseModel):
    chat_id: str = "default"
    text: str = Field(..., min_length=1)


class SettingsUpdateRequest(BaseModel):
    app_username: str | None = None
    app_password: str | None = None
    pansou_base_url: str | None = None
    openlist_base_url: str | None = None
    cloud_types: list[str] | None = None
    check_links: bool | None = None
    probe_quark_files: bool | None = None
    filter_bad_links: bool | None = None
    aria2_rpc_url: str | None = None
    aria2_secret: str | None = None
    aria2_dir: str | None = None


def current_settings() -> dict[str, Any]:
    return settings_store.get()


def simplify_result(item: dict[str, Any], index: int) -> dict[str, Any]:
    return {
        "index": index,
        "title": item.get("note") or "未命名资源",
        "url": item.get("url"),
        "password": item.get("password") or "",
        "source": item.get("source") or "",
        "datetime": item.get("datetime") or "",
        "images": item.get("images") or [],
        "cloud_type": item.get("cloud_type") or "quark",
    }


def format_search_reply(keyword: str, results: list[dict[str, Any]]) -> str:
    if not results:
        return f"没找到《{keyword}》的可用资源。"

    lines = [f"找到《{keyword}》的资源：", ""]
    for item in results:
        title = item["title"].replace("\n", " ").strip()
        source = item.get("source") or "未知来源"
        cloud_type = item.get("cloud_type") or "unknown"
        link_state = item.get("link_check", {}).get("state") if item.get("link_check") else "未检测"
        link_summary = item.get("link_check", {}).get("summary") if item.get("link_check") else ""
        probe = item.get("probe") or {}
        episode = probe.get("episode_count")
        file_count = probe.get("file_count")
        extra = f"，网盘：{cloud_type}，有效性：{link_state}"
        if link_summary:
            extra += f"({link_summary})"
        if file_count is not None:
            extra += f"，文件：{file_count}"
        if episode:
            extra += f"，疑似剧集：{episode}集"
        lines.append(f"{item['index']}. {title}")
        lines.append(f"   来源：{source}{extra}")
    lines.append("")
    lines.append("回复：选 1 / 选 2 / 选 3")
    return "\n".join(lines)


def extract_keyword(text: str) -> str | None:
    text = text.strip()
    patterns = [
        r"^(?:我想看|想看|帮我找|搜索|找一下|找)(.+)$",
        r"^(.+)$",
    ]
    for pattern in patterns:
        m = re.match(pattern, text)
        if m:
            kw = m.group(1).strip(" ：:《》")
            if kw and not re.match(r"^选\s*\d+$", kw):
                return kw
    return None


def extract_selection(text: str) -> int | None:
    m = re.match(r"^选\s*(\d+)$", text.strip())
    if not m:
        return None
    return int(m.group(1))


def enrich_results(results: list[dict[str, Any]], check_links: bool, probe_files: bool, pansou_base_url: str) -> None:
    quark_results = [item for item in results if item.get("cloud_type") == "quark"]
    if check_links and quark_results:
        try:
            checks = PanSouLinkChecker(pansou_base_url).check_quark(quark_results)
            by_url = {c.get("url"): c for c in checks}
            by_norm = {c.get("normalized_url"): c for c in checks if c.get("normalized_url")}
            for item in quark_results:
                item["link_check"] = by_url.get(item.get("url")) or by_norm.get(item.get("url")) or {
                    "state": "unknown",
                    "summary": "未返回检测结果",
                }
        except Exception as e:
            for item in quark_results:
                item["link_check"] = {"state": "error", "summary": str(e)}

    # Non-Quark links are not supported by PanSou check/probe yet.
    for item in results:
        if item.get("cloud_type") != "quark":
            item["link_check"] = {"state": "unsupported", "summary": "暂不支持该网盘检测"}

    if probe_files and quark_results:
        probe = QuarkShareProbe()
        for item in quark_results:
            state = (item.get("link_check") or {}).get("state")
            if state == "bad":
                item["probe"] = {
                    "ok": False,
                    "state": "skipped",
                    "message": "链接检测为失效，跳过嗅探",
                    "files": [],
                    "file_count": 0,
                    "episode_count": 0,
                }
                continue
            info = probe.probe(item.get("url") or "", item.get("password") or "")
            item["probe"] = {
                "ok": info.ok,
                "state": info.state,
                "message": info.message,
                "files": info.files[:80],
                "file_count": info.file_count,
                "episode_count": info.episode_count,
            }


@app.get("/", dependencies=[Depends(require_auth)])
def index():
    return FileResponse("static/index.html")


@app.get("/health")
def health():
    settings = current_settings()
    return {
        "status": "ok",
        "pansou_base_url": settings.get("pansou_base_url"),
        "openlist_base_url": settings.get("openlist_base_url"),
        "auth_enabled": bool(settings.get("app_username") and settings.get("app_password")),
        "check_links": settings.get("check_links"),
        "probe_quark_files": settings.get("probe_quark_files"),
        "filter_bad_links": settings.get("filter_bad_links"),
    }


@app.get("/api/settings", dependencies=[Depends(require_auth)])
def get_settings():
    return settings_store.public()


@app.post("/api/settings", dependencies=[Depends(require_auth)])
def update_settings(req: SettingsUpdateRequest):
    patch = req.model_dump(exclude_unset=True)
    settings_store.update(patch)
    return settings_store.public()


@app.post("/api/aria2/test", dependencies=[Depends(require_auth)])
def test_aria2():
    settings = current_settings()
    client = Aria2Client(settings.get("aria2_rpc_url") or "", settings.get("aria2_secret") or "")
    return {"version": client.get_version()}


@app.post("/api/download/aria2", dependencies=[Depends(require_auth)])
def download_with_aria2(req: DownloadRequest):
    settings = current_settings()
    url = req.url
    item = None
    if req.index is not None:
        sess = sessions.get(req.chat_id)
        if not sess:
            raise HTTPException(status_code=404, detail="没有找到最近的搜索结果，请先搜索。")
        if req.index > len(sess.results):
            raise HTTPException(status_code=400, detail="选择编号超出范围。")
        item = sess.results[req.index - 1]
        url = item.get("url")
    if not url:
        raise HTTPException(status_code=400, detail="缺少下载链接")
    client = Aria2Client(settings.get("aria2_rpc_url") or "", settings.get("aria2_secret") or "")
    gid = client.add_uri([url], req.dir or settings.get("aria2_dir") or "")
    return {"gid": gid, "url": url, "selected": item}


@app.post("/api/search", dependencies=[Depends(require_auth)])
def search(req: SearchRequest):
    settings = current_settings()
    cloud_types = req.cloud_types or settings.get("cloud_types") or ["quark"]
    raw = PanSouClient(settings.get("pansou_base_url")).search(req.keyword, cloud_types, req.limit)
    original_results = [simplify_result(item, i) for i, item in enumerate(raw, 1)]
    results = list(original_results)
    do_check = settings.get("check_links") if req.check_links is None else req.check_links
    do_probe = settings.get("probe_quark_files") if req.probe_files is None else req.probe_files
    do_filter_bad = settings.get("filter_bad_links") if req.filter_bad_links is None else req.filter_bad_links

    enrich_results(results, check_links=bool(do_check), probe_files=bool(do_probe), pansou_base_url=settings.get("pansou_base_url"))

    filtered_count = 0
    if do_filter_bad and do_check:
        kept = []
        for item in results:
            state = (item.get("link_check") or {}).get("state")
            if state == "bad":
                filtered_count += 1
                continue
            kept.append(item)
        results = kept
        for i, item in enumerate(results, 1):
            item["index"] = i

    sessions.set(req.chat_id, req.keyword, results)
    return {
        "keyword": req.keyword,
        "results": results,
        "original_total": len(original_results),
        "filtered_count": filtered_count,
        "reply": format_search_reply(req.keyword, results),
    }


@app.post("/api/select", dependencies=[Depends(require_auth)])
def select(req: SelectRequest):
    sess = sessions.get(req.chat_id)
    if not sess:
        raise HTTPException(status_code=404, detail="没有找到最近的搜索结果，请先搜索。")
    if req.index > len(sess.results):
        raise HTTPException(status_code=400, detail="选择编号超出范围。")
    item = sess.results[req.index - 1]
    return {
        "keyword": sess.keyword,
        "selected": item,
        "reply": (
            f"已选择：{item['title']}\n"
            f"链接：{item['url']}\n\n"
            "可以在 WebUI 点击发送到 Aria2；夸克转存到 /pansou 将在下一阶段接入。"
        ),
    }


@app.post("/api/wechat/message", dependencies=[Depends(require_auth)])
def wechat_message(req: WechatMessageRequest):
    selected = extract_selection(req.text)
    if selected is not None:
        return select(SelectRequest(chat_id=req.chat_id, index=selected))

    keyword = extract_keyword(req.text)
    if not keyword:
        return {"reply": "请发送：想看 电影名，例如：想看 盗梦空间"}
    return search(SearchRequest(chat_id=req.chat_id, keyword=keyword))
