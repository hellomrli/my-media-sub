from __future__ import annotations

from fastapi import Depends, FastAPI
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from .api.routes_downloads import router as downloads_router
from .api.routes_notifications import router as notifications_router
from .api.routes_search import router as search_router
from .api.routes_settings import router as settings_router
from .api.routes_subscriptions import router as subscriptions_router
from .api.routes_wechat import router as wechat_router
from .auth import require_auth
from .services.settings_service import health_payload


def create_app() -> FastAPI:
    app = FastAPI(title="my-media-sub", version="0.3.0")
    app.mount("/static", StaticFiles(directory="static"), name="static")

    @app.get("/", dependencies=[Depends(require_auth)])
    def index():
        return FileResponse("static/index.html")

    @app.get("/health")
    def health():
        return health_payload()

    app.include_router(settings_router)
    app.include_router(subscriptions_router)
    app.include_router(notifications_router)
    app.include_router(downloads_router)
    app.include_router(search_router)
    app.include_router(wechat_router)
    return app


app = create_app()
