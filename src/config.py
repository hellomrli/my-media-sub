import os
from dotenv import load_dotenv

load_dotenv()

QUARK_COOKIE = os.getenv("QUARK_COOKIE", "")
QUARK_SAVE_ROOT = os.getenv("QUARK_SAVE_ROOT", "")
BOT_PORT = int(os.getenv("BOT_PORT", "8787"))
APP_USERNAME = os.getenv("APP_USERNAME", "")
APP_PASSWORD = os.getenv("APP_PASSWORD", "")
PROBE_QUARK_FILES = os.getenv("PROBE_QUARK_FILES", "true").lower() in {"1", "true", "yes", "on"}
CHECK_LINKS = os.getenv("CHECK_LINKS", "true").lower() in {"1", "true", "yes", "on"}
FILTER_BAD_LINKS = os.getenv("FILTER_BAD_LINKS", "true").lower() in {"1", "true", "yes", "on"}
