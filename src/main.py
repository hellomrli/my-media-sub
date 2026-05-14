from __future__ import annotations

import re
from typing import Any

from fastapi import Depends, FastAPI, HTTPException
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel, Field

from . import config
from .auth import require_auth
from .link_checker import PanSouLinkChecker
from .pansou_client import PanSouClient
from .quark_probe import QuarkShareProbe
from .session_store import MemorySessionStore

app = FastAPI(title="my-media-sub", version="0.2.0")
app.mount("/static", StaticFiles(directory="static"), name="static")
pansou = PanSouClient(config.PANSOU_BASE_URL)
link_checker = PanSouLinkChecker(config.PANSOU_BASE_URL)
quark_probe = QuarkShareProbe(config.QUARK_COOKIE)
sessions = MemorySessionStore()


class SearchRequest(BaseModel):
    keyword: str = Field(..., min_length=1)
    chat_id: str = "default"
    limit: int = Field(default=8, ge=1, le=20)
    check_links: bool | None = None
    probe_files: bool | None = None


class SelectRequest(BaseModel):
    chat_id: str = "default"
    index: int = Field(..., ge=1)


class WechatMessageRequest(BaseModel):
    chat_id: str = "default"
    text: str = Field(..., min_length=1)


def simplify_result(item: dict[str, Any], index: int) -> dict[str, Any]:
    return {
        "index": index,
        "title": item.get("note") or "未命名资源",
        "url": item.get("url"),
        "password": item.get("password") or "",
        "source": item.get("source") or "",
        "datetime": item.get("datetime") or "",
        "images": item.get("images") or [],
    }


def format_search_reply(keyword: str, results: list[dict[str, Any]]) -> str:
    if not results:
        return f"没找到《{keyword}》的夸克资源。"

    lines = [f"找到《{keyword}》的夸克资源：", ""]
    for item in results:
        title = item["title"].replace("\n", " ").strip()
        source = item.get("source") or "未知来源"
        link_state = item.get("link_check", {}).get("state") if item.get("link_check") else "未检测"
        link_summary = item.get("link_check", {}).get("summary") if item.get("link_check") else ""
        probe = item.get("probe") or {}
        episode = probe.get("episode_count")
        file_count = probe.get("file_count")
        extra = f"，有效性：{link_state}"
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


def enrich_results(results: list[dict[str, Any]], check_links: bool, probe_files: bool) -> None:
    if check_links and results:
        try:
            checks = link_checker.check_quark(results)
            by_url = {c.get("url"): c for c in checks}
            by_norm = {c.get("normalized_url"): c for c in checks if c.get("normalized_url")}
            for item in results:
                item["link_check"] = by_url.get(item.get("url")) or by_norm.get(item.get("url")) or {
                    "state": "unknown",
                    "summary": "未返回检测结果",
                }
        except Exception as e:
            for item in results:
                item["link_check"] = {"state": "error", "summary": str(e)}

    if probe_files and results:
        for item in results:
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
            info = quark_probe.probe(item.get("url") or "", item.get("password") or "")
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
    return {
        "status": "ok",
        "pansou_base_url": config.PANSOU_BASE_URL,
        "openlist_base_url": config.OPENLIST_BASE_URL,
        "auth_enabled": bool(config.APP_USERNAME and config.APP_PASSWORD),
        "check_links": config.CHECK_LINKS,
        "probe_quark_files": config.PROBE_QUARK_FILES,
    }


@app.post("/api/search", dependencies=[Depends(require_auth)])
def search(req: SearchRequest):
    raw = pansou.search_quark(req.keyword, req.limit)
    results = [simplify_result(item, i) for i, item in enumerate(raw, 1)]
    do_check = config.CHECK_LINKS if req.check_links is None else req.check_links
    do_probe = config.PROBE_QUARK_FILES if req.probe_files is None else req.probe_files
    enrich_results(results, check_links=do_check, probe_files=do_probe)
    sessions.set(req.chat_id, req.keyword, results)
    return {
        "keyword": req.keyword,
        "results": results,
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
    # TODO: 接入夸克转存服务 + OpenList/NAS 下载。
    return {
        "keyword": sess.keyword,
        "selected": item,
        "reply": (
            f"已选择：{item['title']}\n"
            f"链接：{item['url']}\n\n"
            "下一步将接入夸克转存到 /pansou，然后由 OpenList/NAS 处理。"
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
