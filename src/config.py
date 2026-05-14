import os
from dotenv import load_dotenv

load_dotenv()

PANSOU_BASE_URL = os.getenv("PANSOU_BASE_URL", "https://pansou.lxf87.com.cn")
OPENLIST_BASE_URL = os.getenv("OPENLIST_BASE_URL", "https://pan.lxf87.com.cn")
OPENLIST_USERNAME = os.getenv("OPENLIST_USERNAME", "")
OPENLIST_PASSWORD = os.getenv("OPENLIST_PASSWORD", "")
OPENLIST_TOKEN = os.getenv("OPENLIST_TOKEN", "")
QUARK_COOKIE = os.getenv("QUARK_COOKIE", "")
QUARK_SAVE_ROOT = os.getenv("QUARK_SAVE_ROOT", "/pansou")
BOT_PORT = int(os.getenv("BOT_PORT", "8787"))
APP_USERNAME = os.getenv("APP_USERNAME", "")
APP_PASSWORD = os.getenv("APP_PASSWORD", "")
PROBE_QUARK_FILES = os.getenv("PROBE_QUARK_FILES", "true").lower() in {"1", "true", "yes", "on"}
CHECK_LINKS = os.getenv("CHECK_LINKS", "true").lower() in {"1", "true", "yes", "on"}
FILTER_BAD_LINKS = os.getenv("FILTER_BAD_LINKS", "true").lower() in {"1", "true", "yes", "on"}
