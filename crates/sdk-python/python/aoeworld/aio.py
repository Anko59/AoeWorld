"""asyncio helpers. The native waits release the GIL, so one executor thread
can serve a whole fleet of bots while the event loop stays responsive."""

from __future__ import annotations

import asyncio
from concurrent.futures import ThreadPoolExecutor
from typing import AsyncIterator, Optional

from ._native import Decision, Fleet


class AsyncFleet:
    """`async for index, decision in fleet.decisions()` over many connections.

    ``decision`` is ``None`` exactly once for a connection that has closed;
    the fleet then ignores it.
    """

    def __init__(self, fleet: Fleet, poll_seconds: float = 0.5) -> None:
        self._fleet = fleet
        self._poll = poll_seconds
        # One dedicated waiter thread: all bots share it.
        self._executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix="aoeworld-wait")

    async def wait(self, timeout: Optional[float] = None) -> list[tuple[int, Optional[Decision]]]:
        loop = asyncio.get_running_loop()
        return await loop.run_in_executor(
            self._executor, self._fleet.wait, self._poll if timeout is None else timeout
        )

    async def decisions(self) -> AsyncIterator[tuple[int, Optional[Decision]]]:
        while True:
            for item in await self.wait():
                yield item

    def close(self) -> None:
        self._executor.shutdown(wait=False, cancel_futures=True)
