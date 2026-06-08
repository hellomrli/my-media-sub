from __future__ import annotations

from contextlib import asynccontextmanager

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
from .services.scheduler_service import start_scheduler, stop_scheduler
from .services.settings_service import health_payload


@asynccontextmanager
async def lifespan(app: FastAPI):
    start_scheduler()
    try:
        yield
    finally:
        await stop_scheduler()


def create_app() -> FastAPI:
    app = FastAPI(title="my-media-sub", version="0.3.0", lifespan=lifespan)
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
