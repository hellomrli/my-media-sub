#!/usr/bin/env python3
"""
Minimal test server for v0.4.0 new features.
Only includes new async services, no old dependencies.
"""

from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.responses import JSONResponse, FileResponse
from fastapi.staticfiles import StaticFiles

from src.config_new import settings
from src.database import init_db
from src.task_queue import task_queue


@asynccontextmanager
async def lifespan(app: FastAPI):
    # Initialize database
    print("📦 Initializing database...")
    await init_db()
    
    # Start task queue
    print("🔄 Starting task queue...")
    await task_queue.start()
    
    print("✅ Server ready!")
    
    try:
        yield
    finally:
        print("🛑 Shutting down...")
        await task_queue.stop()


app = FastAPI(
    title="my-media-sub v0.4.0 (New Features Only)",
    version="0.4.0",
    description="Test server with only new v0.4.0 features",
    lifespan=lifespan,
)

app.mount("/static", StaticFiles(directory="static"), name="static")


@app.get("/")
async def root():
    return FileResponse("static/test.html")


@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "version": "0.4.0",
        "database": settings.database_url,
        "queue": task_queue.status(),
    }


@app.get("/api/queue/status")
async def queue_status():
    return task_queue.status()


@app.post("/api/test/search")
async def test_search(keyword: str):
    """Test async search with progress and filtering."""
    from src.clients.pansou_async import InlinePanSouClientAsync
    from src.utils.deduplication import deduplicate_results, enhance_results_with_quality
    
    client = InlinePanSouClientAsync()
    results = await client.search_quark(keyword, limit=10)
    
    # Filter out invalid links (404, expired)
    valid_results = [
        r for r in results 
        if not (r.get('validity_status') == 'http_error' or 
                'status":404' in str(r.get('validity_info', '')) or
                '已失效' in str(r.get('validity_info', '')))
    ]
    
    # Apply deduplication
    enhanced = enhance_results_with_quality(valid_results)
    deduplicated = deduplicate_results(enhanced)
    
    return {
        "keyword": keyword,
        "total_results": len(results),
        "valid_results": len(valid_results),
        "filtered_out": len(results) - len(valid_results),
        "deduplicated_results": len(deduplicated),
        "results": deduplicated[:5],  # Return first 5
    }


@app.get("/api/test/db")
async def test_db():
    """Test database connection."""
    from src.database import async_session, Subscription
    from sqlalchemy import select, func
    
    async with async_session() as session:
        result = await session.execute(select(func.count()).select_from(Subscription))
        count = result.scalar()
    
    return {
        "status": "ok",
        "subscriptions_count": count,
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8788)
