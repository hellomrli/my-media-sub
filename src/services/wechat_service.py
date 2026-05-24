from __future__ import annotations

import re
from typing import Any

from .search_service import search_media, select_result


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


def handle_wechat_message(chat_id: str, text: str) -> dict[str, Any]:
    selected = extract_selection(text)
    if selected is not None:
        return select_result(chat_id, selected)

    keyword = extract_keyword(text)
    if not keyword:
        return {"reply": "请发送：想看 电影名，例如：想看 盗梦空间"}
    return search_media(
        keyword=keyword,
        chat_id=chat_id,
        limit=8,
        cloud_types=None,
        check_links=None,
        probe_files=None,
        filter_bad_links=None,
    )
