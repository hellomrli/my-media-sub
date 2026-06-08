from __future__ import annotations

import logging
from typing import Any

from ..clients.quark import QuarkShareProbe
from ..clients.quark_save import QuarkSaveClient
from ..stores.notification_store import notification_store
from ..stores.settings_store import settings_store

logger = logging.getLogger(__name__)


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
    save_root = (settings.get("quark_save_root") or "").strip()
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

    # 3. Collect fids + tokens for transfer items
    fid_list: list[str] = []
    fid_token_list: list[str] = []
    saved_files: list[dict[str, Any]] = []
    missing: list[str] = []

    for t in transfers:
        name = t.get("source_name") or ""
        entry = share_file_map.get(name)
        if entry and entry["fid"] and entry["share_fid_token"]:
            fid_list.append(entry["fid"])
            fid_token_list.append(entry["share_fid_token"])
            saved_files.append({"name": name, "fid": entry["fid"]})
        else:
            missing.append(name)

    if not fid_list:
        if missing:
            notification_store.add("warning", "quark_save_failed", f"夸克转存失败：{title}", f"分享列表中找不到 fid_token：{', '.join(missing[:5])}", {"subscription_id": sub.get("id")})
        return []

    # 4. Find/create target directory
    save_client = QuarkSaveClient(cookie=cookie)
    target_fid = "0"
    if save_root:
        try:
            target_fid = save_client.ensure_dir_path(save_root)
        except Exception as exc:
            logger.warning("Failed to resolve Quark save dir '%s': %s", save_root, exc)
            target_fid = "0"

    # 5. Execute save
    try:
        result = save_client.save_share_files(pwd_id, stoken, fid_list, fid_token_list, target_fid)
        code = result.get("code", -1)
        if code != 0:
            msg = result.get("message") or result.get("msg") or f"未知错误(code={code})"
            notification_store.add("warning", "quark_save_failed", f"夸克转存失败：{title}", msg, {"subscription_id": sub.get("id")})
            return []
    except Exception as exc:
        notification_store.add("warning", "quark_save_failed", f"夸克转存失败：{title}", str(exc), {"subscription_id": sub.get("id")})
        return []

    # Mark transferred in subscription store
    from ..stores.subscription_store import subscription_store
    subscription_store.mark_transferred(sub["id"], transfers)

    notification_store.add(
        "info",
        "quark_save_success",
        f"夸克转存成功：{title}",
        f"已转存 {len(saved_files)} 个文件到夸克网盘。",
        {"subscription_id": sub.get("id"), "files": saved_files, "target_fid": target_fid},
    )
    return [{"name": t.get("source_name"), "fid": t.get("fid") or entry.get("fid"), "status": "saved"} for t in transfers if (entry := share_file_map.get(t.get("source_name") or ""))]
