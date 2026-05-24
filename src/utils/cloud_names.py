CLOUD_TYPE_NAMES = {
    "quark": "夸克网盘",
    "baidu": "百度网盘",
    "aliyun": "阿里云盘",
    "uc": "UC网盘",
    "tianyi": "天翼云盘",
    "mobile": "移动云盘",
    "115": "115网盘",
    "pikpak": "PikPak",
    "xunlei": "迅雷网盘",
    "123": "123网盘",
    "magnet": "磁力链接",
    "ed2k": "电驴链接",
    "others": "其他资源",
}


def cloud_name(cloud_type: str | None) -> str:
    if not cloud_type:
        return "未知网盘"
    return CLOUD_TYPE_NAMES.get(cloud_type, cloud_type)
