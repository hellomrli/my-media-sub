#!/usr/bin/env python3
"""
Quick test script for v0.4.0 improvements.

Usage:
    python test_improvements.py
"""

import asyncio
import time


async def test_async_search():
    """Test async search performance."""
    print("\n🔍 Testing async search...")
    from src.clients.pansou_async import InlinePanSouClientAsync
    
    client = InlinePanSouClientAsync()
    keyword = "庆余年"
    
    start = time.time()
    results = await client.search_quark(keyword, limit=20)
    elapsed = time.time() - start
    
    print(f"  ✅ Found {len(results)} results in {elapsed:.2f}s")
    if results:
        print(f"  📝 Sample: {results[0].get('note', 'N/A')}")
    return elapsed


async def test_deduplication():
    """Test smart deduplication."""
    print("\n🎨 Testing deduplication...")
    from src.utils.deduplication import deduplicate_results, enhance_results_with_quality
    
    # Mock results
    results = [
        {"title": "电影名 2024 4K", "url": "http://example.com/1"},
        {"title": "电影名 2024 1080p", "url": "http://example.com/2"},
        {"title": "电影名 (2024) 4K", "url": "http://example.com/3"},  # Similar to first
        {"title": "完全不同的电影", "url": "http://example.com/4"},
    ]
    
    enhanced = enhance_results_with_quality(results)
    deduplicated = deduplicate_results(enhanced)
    
    print(f"  📊 Original: {len(results)} results")
    print(f"  📊 After dedup: {len(deduplicated)} results")
    for item in deduplicated:
        print(f"    - {item['title']} (quality: {item.get('quality', 'unknown')})")
    
    assert len(deduplicated) == 2, "Should deduplicate similar titles"
    print("  ✅ Deduplication works!")


async def test_database():
    """Test database operations."""
    print("\n🗄️ Testing database...")
    from src.database import init_db, async_session, Subscription
    
    # Initialize
    await init_db()
    print("  ✅ Database initialized")
    
    # Create test subscription
    async with async_session() as session:
        sub = Subscription(
            id="test-sub-001",
            keyword="测试",
            url="https://pan.quark.cn/s/test123",
            password="",
            media_type="series",
        )
        session.add(sub)
        await session.commit()
        print("  ✅ Created test subscription")
    
    # Query
    from sqlalchemy import select
    async with async_session() as session:
        result = await session.execute(
            select(Subscription).where(Subscription.id == "test-sub-001")
        )
        found = result.scalar_one_or_none()
        assert found is not None, "Should find subscription"
        print(f"  ✅ Found subscription: {found.keyword}")
        
        # Cleanup
        await session.delete(found)
        await session.commit()
        print("  ✅ Cleaned up test data")


async def test_task_queue():
    """Test task queue."""
    print("\n🔄 Testing task queue...")
    from src.task_queue import TaskQueue
    
    queue = TaskQueue(max_workers=2)
    await queue.start()
    
    # Add test tasks
    async def test_task(n):
        await asyncio.sleep(0.1)
        return f"Task {n} done"
    
    for i in range(5):
        await queue.put(f"task-{i}", test_task(i), priority=i)
    
    # Wait for completion
    await asyncio.sleep(1)
    
    status = queue.status()
    print(f"  📊 Queue status: {status}")
    print(f"  ✅ Completed: {status['completed']}/{status['total']}")
    
    await queue.stop()


async def test_config():
    """Test configuration."""
    print("\n⚙️ Testing configuration...")
    from src.config import settings
    
    print(f"  📝 Database URL: {settings.database_url}")
    print(f"  📝 Auto complete after: {settings.auto_complete_after_no_updates} checks")
    print(f"  📝 Quality order: {settings.preferred_quality_order}")
    print("  ✅ Configuration loaded")


async def main():
    """Run all tests."""
    print("=" * 60)
    print("🧪 my-media-sub v0.4.0 Improvements Test")
    print("=" * 60)
    
    try:
        # Test config (fast)
        await test_config()
        
        # Test database (fast)
        await test_database()
        
        # Test deduplication (fast)
        await test_deduplication()
        
        # Test task queue (fast)
        await test_task_queue()
        
        # Test async search (slow - requires network)
        print("\n⚠️  Skipping async search test (requires network and user input)")
        print("   To test manually: python -c 'import asyncio; from src.clients.pansou_async import InlinePanSouClientAsync; asyncio.run(InlinePanSouClientAsync().search_quark(\"庆余年\", 10))'")
        
        print("\n" + "=" * 60)
        print("✅ All tests passed!")
        print("=" * 60)
        
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        import traceback
        traceback.print_exc()
        return 1
    
    return 0


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    exit(exit_code)
