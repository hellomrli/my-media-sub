from __future__ import annotations

import logging
import time
from collections import defaultdict
from typing import Any

from ..clients.quark import QuarkShareProbe
from ..clients.quark_save import QuarkSaveClient
from ..stores.notification_store import notification_store
from ..stores.settings_store import settings_store

logger = logging.getLogger(__name__)


def _list_dir_until_names(client: QuarkSaveClient, target_fid: str, names: set[str], attempts: int = 6, delay: float = 1.5) -> dict[str, dict[str, Any]]:
    by_name: dict[str, dict[str, Any]] = {}
    for attempt in range(attempts):
        items = client.list_dir(target_fid)
        by_name = {item.get("file_name") or item.get("name") or "": item for item in items}
        if names & set(by_name):
            return by_name
        if attempt < attempts - 1:
            time.sleep(delay)
    return by_name


def save_subscription_transfers(
    updated: dict[str, Any] | None,
    plan: dict[str, Any],
) -> list[dict[str, Any]]:
    """Auto-save newly discovered subscription items to the user's Quark drive.

    Returns a list of save results (empty list if nothing was saved or if
    conditions are not met).

    Conditions:
    - Subscription exists and notify_only is False.
    - Global 'quark_save_enabled' setting is True.
    - Quark cookie is configured.
    - There are transfers in the plan.
    """
    if not updated or updated.get("notify_only"):
        return []
    settings = settings_store.get()
    if not settings.get("quark_save_enabled"):
        return []
    cookie = settings.get("quark_cookie") or ""
    
    # 基础目录前缀
    base_root = (settings.get("quark_save_root") or "").strip()
    
    # 根据订阅的 media_type 选择分类子目录
    media_type = sub.get("media_type", "series")
    
    # 默认分类
    default_dirs = {
        "movie": settings.get("quark_save_movie_dir", "/电影"),
        "series": settings.get("quark_save_series_dir", "/连续剧"),
        "anime": settings.get("quark_save_anime_dir", "/动画"),
    }
    
    # 自定义分类
    custom_categories = settings.get("custom_categories", [])
    for cat in custom_categories:
        if cat.get("name"):
            default_dirs[f"custom_{cat['name']}"] = cat.get("dir", "")
    
    category_dir = (default_dirs.get(media_type) or "").strip()
    
    # 组合：基础目录 + 分类目录
    save_root = "/".join(p.strip("/") for p in [base_root, category_dir] if p.strip("/"))
    
    if not cookie:
        return []

    transfers = plan.get("transfers") or []
    if not transfers:
        return []

    sub = updated
    share_url = sub.get("url") or ""
    passcode = sub.get("password") or ""
    title = sub.get("title") or "未命名订阅"

    # 1. Get share access info
    probe = QuarkShareProbe(cookie=cookie)
    pwd_id = probe.extract_pwd_id(share_url)
    if not pwd_id:
        notification_store.add("warning", "quark_save_failed", f"夸克转存失败：{title}", "无效的分享链接", {"subscription_id": sub.get("id")})
        return []

    stoken, err = probe.get_share_token(pwd_id, passcode)
    if err or not stoken:
        notification_store.add("warning", "quark_save_failed", f"夸克转存失败：{title}", f"获取分享 token 失败：{err}", {"subscription_id": sub.get("id")})
        return []

    # 2. List share files to get fid_tokens for the transfers
    raw_list, list_err = probe.list_files(pwd_id, stoken)
    if list_err or not raw_list:
        notification_store.add("warning", "quark_save_failed", f"夸克转存失败：{title}", f"列出分享文件失败：{list_err}", {"subscription_id": sub.get("id")})
        return []

    # Build a lookup: source_name -> {fid, share_fid_token}
    share_file_map: dict[str, dict[str, Any]] = {}
    queue = list(raw_list)
    while queue:
        item = queue.pop(0)
        name = item.get("file_name") or item.get("name") or ""
        fid = item.get("fid") or item.get("file_id") or ""
        token = item.get("share_fid_token") or item.get("file_token") or ""
        is_dir = bool(item.get("dir") or item.get("file") is False or (item.get("file_type") == 0 and not item.get("format_type") and item.get("size", 0) == 0))
        if name and fid and token:
            share_file_map[name] = {"fid": fid, "share_fid_token": token, "is_dir": is_dir}
        if is_dir and fid:
            children, _ = probe.list_files(pwd_id, stoken, pdir_fid=fid)
            queue.extend(children)

    # 3. Collect fids + tokens for transfer items and group them by target directory.
    grouped_transfers: dict[str, list[dict[str, Any]]] = defaultdict(list)
    missing: list[str] = []

    for t in transfers:
        name = t.get("source_name") or ""
        entry = share_file_map.get(name)
        if entry and entry["fid"] and entry["share_fid_token"]:
            enriched = dict(t)
            enriched["source_fid"] = entry["fid"]
            enriched["share_fid_token"] = entry["share_fid_token"]
            grouped_transfers[t.get("target_dir") or plan.get("target_dir") or ""].append(enriched)
        else:
            missing.append(name)

    if not grouped_transfers:
        if missing:
            notification_store.add("warning", "quark_save_failed", f"夸克转存失败：{title}", f"分享列表中找不到 fid_token：{', '.join(missing[:5])}", {"subscription_id": sub.get("id")})
        return []

    # 4. Execute save per target directory, then apply target names.
    save_client = QuarkSaveClient(cookie=cookie)
    saved_files: list[dict[str, Any]] = []
    target_fids: dict[str, str] = {}

    for target_dir, items in grouped_transfers.items():
        relative_target_dir = (target_dir or "").strip("/")
        full_target_dir = "/".join(part.strip("/") for part in [save_root, relative_target_dir] if part and part.strip("/"))
        try:
            target_fid = save_client.ensure_dir_path(full_target_dir) if full_target_dir else "0"
        except Exception as exc:
            notification_store.add("warning", "quark_save_failed", f"夸克转存失败：{title}", f"创建目标目录失败：/{full_target_dir}：{exc}", {"subscription_id": sub.get("id")})
            continue
        target_fids[target_dir or "/"] = target_fid

        fid_list = [item["source_fid"] for item in items]
        fid_token_list = [item["share_fid_token"] for item in items]
        try:
            result = save_client.save_share_files(pwd_id, stoken, fid_list, fid_token_list, target_fid)
            code = result.get("code", -1)
            if code != 0:
                msg = result.get("message") or result.get("msg") or f"未知错误(code={code})"
                notification_store.add("warning", "quark_save_failed", f"夸克转存失败：{title}", msg, {"subscription_id": sub.get("id"), "target_dir": target_dir})
                continue
        except Exception as exc:
            notification_store.add("warning", "quark_save_failed", f"夸克转存失败：{title}", str(exc), {"subscription_id": sub.get("id"), "target_dir": target_dir})
            continue

        expected_names = {item.get("source_name") or "" for item in items} | {item.get("target_name") or "" for item in items}
        expected_names.discard("")
        by_name = _list_dir_until_names(save_client, target_fid, expected_names)
        for item in items:
            source_name = item.get("source_name") or ""
            target_name = item.get("target_name") or source_name
            saved = by_name.get(source_name) or by_name.get(target_name)
            saved_fid = (saved or {}).get("fid") or (saved or {}).get("file_id") or ""
            if saved_fid and target_name and target_name != source_name:
                try:
                    rename_result = save_client.rename_item(saved_fid, target_name)
                    if rename_result.get("code", 0) != 0:
                        logger.warning("Failed to rename Quark item %s to %s: %s", source_name, target_name, rename_result)
                    else:
                        source_name = target_name
                except Exception as exc:
                    logger.warning("Failed to rename Quark item %s to %s: %s", source_name, target_name, exc)
            saved_files.append({"name": item.get("source_name"), "target_name": target_name, "fid": saved_fid, "target_dir": target_dir})

    if not saved_files:
        return []

    # Mark transferred in subscription store
    refreshed_cookie = save_client.cookie or probe.cookie
    if refreshed_cookie and refreshed_cookie != cookie:
        settings_store.update_secret("quark_cookie", refreshed_cookie)
        logger.info("Quark cookie refreshed after save for subscription %s", sub.get("id"))

    from ..stores.subscription_store import subscription_store
    successful_source_names = {item.get("name") for item in saved_files}
    subscription_store.mark_transferred(sub["id"], [t for t in transfers if t.get("source_name") in successful_source_names])

    notification_store.add(
        "info",
        "quark_save_success",
        f"夸克转存成功：{title}",
        f"已转存 {len(saved_files)} 个文件到夸克网盘。",
        {"subscription_id": sub.get("id"), "files": saved_files, "target_fids": target_fids},
    )
    return [{"name": item.get("name"), "target_name": item.get("target_name"), "fid": item.get("fid"), "target_dir": item.get("target_dir"), "status": "saved"} for item in saved_files]
