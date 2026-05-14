from __future__ import annotations

import re
from typing import Any

from fastapi import FastAPI, HTTPException
from fastapi.staticfiles import StaticFiles
from fastapi.responses import FileResponse
from pydantic import BaseModel, Field

from . import config
from .pansou_client import PanSouClient
from .session_store import MemorySessionStore

app = FastAPI(title="my-media-sub", version="0.1.0")
app.mount("/static", StaticFiles(directory="static"), name="static")
pansou = PanSouClient(config.PANSOU_BASE_URL)
sessions = MemorySessionStore()


class SearchRequest(BaseModel):
    keyword: str = Field(..., min_length=1)
    chat_id: str = "default"
    limit: int = Field(default=8, ge=1, le=20)


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
        lines.append(f"{item['index']}. {title}")
        lines.append(f"   来源：{source}")
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


@app.get("/")
def index():
    return FileResponse("static/index.html")


@app.get("/health")
def health():
    return {
        "status": "ok",
        "pansou_base_url": config.PANSOU_BASE_URL,
        "openlist_base_url": config.OPENLIST_BASE_URL,
    }


@app.post("/api/search")
def search(req: SearchRequest):
    raw = pansou.search_quark(req.keyword, req.limit)
    results = [simplify_result(item, i) for i, item in enumerate(raw, 1)]
    sessions.set(req.chat_id, req.keyword, results)
    return {
        "keyword": req.keyword,
        "results": results,
        "reply": format_search_reply(req.keyword, results),
    }


@app.post("/api/select")
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


@app.post("/api/wechat/message")
def wechat_message(req: WechatMessageRequest):
    selected = extract_selection(req.text)
    if selected is not None:
        return select(SelectRequest(chat_id=req.chat_id, index=selected))

    keyword = extract_keyword(req.text)
    if not keyword:
        return {"reply": "请发送：想看 电影名，例如：想看 盗梦空间"}
    return search(SearchRequest(chat_id=req.chat_id, keyword=keyword))
