import os

def env_bool(name: str, default: str = "false") -> bool:
    return os.getenv(name, default).lower() in {"1", "true", "yes", "on"}

APP_USERNAME = os.getenv("APP_USERNAME", "")
APP_PASSWORD = os.getenv("APP_PASSWORD", "")
CHECK_LINKS = env_bool("CHECK_LINKS", "true")
PROBE_QUARK_FILES = env_bool("PROBE_QUARK_FILES", "true")
FILTER_BAD_LINKS = env_bool("FILTER_BAD_LINKS", "true")
