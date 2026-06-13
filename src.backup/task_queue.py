from __future__ import annotations

import asyncio
import logging
from collections.abc import Coroutine
from dataclasses import dataclass
from datetime import datetime
from typing import Any

logger = logging.getLogger(__name__)


@dataclass
class Task:
    id: str
    coro: Coroutine
    priority: int = 0
    created_at: datetime = None

    def __post_init__(self):
        if self.created_at is None:
            self.created_at = datetime.now()


class TaskQueue:
    """Async task queue with priority and retry support."""

    def __init__(self, max_workers: int = 3):
        self.queue: asyncio.PriorityQueue = asyncio.PriorityQueue()
        self.max_workers = max_workers
        self.workers: list[asyncio.Task] = []
        self.running = False
        self.task_count = 0
        self.completed_count = 0
        self.failed_count = 0

    async def put(self, task_id: str, coro: Coroutine, priority: int = 0):
        """Add a task to the queue."""
        task = Task(id=task_id, coro=coro, priority=priority)
        await self.queue.put((priority, self.task_count, task))
        self.task_count += 1
        logger.info(f"Task {task_id} added to queue (priority={priority})")

    async def _worker(self, worker_id: int):
        """Worker coroutine that processes tasks from the queue."""
        logger.info(f"Worker {worker_id} started")
        while self.running:
            try:
                # Wait for task with timeout to allow graceful shutdown
                try:
                    _, _, task = await asyncio.wait_for(self.queue.get(), timeout=1.0)
                except TimeoutError:
                    continue

                logger.info(f"Worker {worker_id} processing task {task.id}")
                try:
                    await task.coro
                    self.completed_count += 1
                    logger.info(f"Task {task.id} completed successfully")
                except Exception as e:
                    self.failed_count += 1
                    logger.error(f"Task {task.id} failed: {e}", exc_info=True)
                finally:
                    self.queue.task_done()
            except Exception as e:
                logger.error(f"Worker {worker_id} error: {e}", exc_info=True)

        logger.info(f"Worker {worker_id} stopped")

    async def start(self):
        """Start the task queue workers."""
        if self.running:
            return

        self.running = True
        self.workers = [
            asyncio.create_task(self._worker(i))
            for i in range(self.max_workers)
        ]
        logger.info(f"Task queue started with {self.max_workers} workers")

    async def stop(self):
        """Stop the task queue workers."""
        self.running = False
        await asyncio.gather(*self.workers, return_exceptions=True)
        self.workers = []
        logger.info("Task queue stopped")

    def status(self) -> dict[str, Any]:
        """Get queue status."""
        return {
            "running": self.running,
            "workers": len(self.workers),
            "queued": self.queue.qsize(),
            "total": self.task_count,
            "completed": self.completed_count,
            "failed": self.failed_count,
        }


# Global task queue instance
task_queue = TaskQueue(max_workers=3)
