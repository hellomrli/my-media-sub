from __future__ import annotations

from typing import Any

from ..clients.link_checker import PanSouLinkChecker
from ..clients.pansou import PanSouClient
from ..clients.quark import QuarkShareProbe
from ..stores.session_store import MemorySessionStore
from ..stores.settings_store import settings_store
from ..utils.cloud_names import cloud_name
from ..utils.download_capabilities import capability_for

sessions = MemorySessionStore()


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
        "cloud_name": cloud_name(item.get("cloud_type") or "quark"),
        "download_capability": capability_for(item.get("cloud_type") or "quark", item.get("url")),
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
        extra = f"，网盘：{cloud_name(cloud_type)}，有效性：{link_state}"
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


def search_media(keyword: str, chat_id: str, limit: int, cloud_types: list[str] | None, check_links: bool | None, probe_files: bool | None, filter_bad_links: bool | None) -> dict[str, Any]:
    settings = current_settings()
    selected_cloud_types = cloud_types or settings.get("cloud_types") or ["quark"]
    raw = PanSouClient(settings.get("pansou_base_url")).search(keyword, selected_cloud_types, limit)
    original_results = [simplify_result(item, i) for i, item in enumerate(raw, 1)]
    results = list(original_results)
    do_check = settings.get("check_links") if check_links is None else check_links
    do_probe = settings.get("probe_quark_files") if probe_files is None else probe_files
    do_filter_bad = settings.get("filter_bad_links") if filter_bad_links is None else filter_bad_links

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

    sessions.set(chat_id, keyword, results)
    return {
        "keyword": keyword,
        "results": results,
        "original_total": len(original_results),
        "filtered_count": filtered_count,
        "reply": format_search_reply(keyword, results),
    }


def select_result(chat_id: str, index: int) -> dict[str, Any]:
    sess = sessions.get(chat_id)
    if not sess:
        raise LookupError("没有找到最近的搜索结果，请先搜索。")
    if index > len(sess.results):
        raise ValueError("选择编号超出范围。")
    item = sess.results[index - 1]
    return {
        "keyword": sess.keyword,
        "selected": item,
        "reply": (
            f"已选择：{item['title']}\n"
            f"链接：{item['url']}\n\n"
            "可以在 WebUI 点击发送到 Aria2；夸克转存到 /pansou 将在下一阶段接入。"
        ),
    }
