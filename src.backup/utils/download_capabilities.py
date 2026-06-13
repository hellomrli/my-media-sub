from __future__ import annotations

DIRECT_ARIA2_TYPES = {"magnet", "ed2k"}
DIRECT_HTTP_PREFIXES = ("http://", "https://")

CAPABILITIES = {
    "magnet": {"action": "aria2", "label": "可直接 Aria2", "direct_aria2": True},
    "ed2k": {"action": "aria2", "label": "可直接 Aria2", "direct_aria2": True},
    "quark": {"action": "save_subscribe", "label": "需转存，可订阅", "direct_aria2": False},
    "uc": {"action": "save_subscribe", "label": "需转存，可订阅", "direct_aria2": False},
    "aliyun": {"action": "save", "label": "需转存/取直链", "direct_aria2": False},
    "baidu": {"action": "save", "label": "需转存/解析", "direct_aria2": False},
    "115": {"action": "save", "label": "需转存/解析", "direct_aria2": False},
    "tianyi": {"action": "save", "label": "需转存/解析", "direct_aria2": False},
    "mobile": {"action": "save", "label": "需转存/解析", "direct_aria2": False},
    "pikpak": {"action": "parse", "label": "需解析/登录", "direct_aria2": False},
    "xunlei": {"action": "parse", "label": "需解析/登录", "direct_aria2": False},
    "123": {"action": "parse", "label": "需解析/取直链", "direct_aria2": False},
    "others": {"action": "copy", "label": "请人工判断", "direct_aria2": False},
}


def capability_for(cloud_type: str | None, url: str | None = None) -> dict:
    cloud_type = cloud_type or "others"
    cap = dict(CAPABILITIES.get(cloud_type, CAPABILITIES["others"]))
    if cloud_type in DIRECT_ARIA2_TYPES:
        return cap
    # Only treat non-pan direct-looking links as Aria2 candidates.
    if cloud_type == "others" and url and url.startswith(DIRECT_HTTP_PREFIXES):
        cap.update({"action": "aria2", "label": "可能是直链，可试 Aria2", "direct_aria2": True})
    return cap
