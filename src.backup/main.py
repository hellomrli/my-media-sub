from __future__ import annotations

# Backward-compatible ASGI entrypoint. New code should import src.app:app.
from .app import app, create_app

__all__ = ["app", "create_app"]
