from __future__ import annotations

import secrets
from fastapi import Depends, HTTPException, Request, status
from fastapi.security import HTTPBasic, HTTPBasicCredentials

from .settings_store import settings_store

security = HTTPBasic(auto_error=False)


def auth_enabled() -> bool:
    s = settings_store.get()
    return bool(s.get("app_username") and s.get("app_password"))


def require_auth(request: Request, credentials: HTTPBasicCredentials | None = Depends(security)):
    """Protect UI and API with HTTP Basic when APP_USERNAME/APP_PASSWORD are set."""
    if not auth_enabled():
        return True

    if credentials is None:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Authentication required",
            headers={"WWW-Authenticate": "Basic"},
        )

    s = settings_store.get()
    username_ok = secrets.compare_digest(credentials.username, s.get("app_username") or "")
    password_ok = secrets.compare_digest(credentials.password, s.get("app_password") or "")
    if not (username_ok and password_ok):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid username or password",
            headers={"WWW-Authenticate": "Basic"},
        )
    return True
