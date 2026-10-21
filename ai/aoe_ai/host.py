"""Many bots, one process: a launcher around :class:`aoeworld.Fleet`.

One thread serves every connection: the native wait releases the GIL, wakes
when any bot has a decision due, and returns all that are due together.
"""

from __future__ import annotations

import asyncio
import logging
import time
from dataclasses import dataclass, field
from typing import Callable, Optional, Sequence

import aoeworld

from .brain import Brain
from .config import Config

log = logging.getLogger("aoe_ai.host")


@dataclass
class BotReport:
    """How one bot did."""

    slot: int
    player: Optional[int] = None
    decisions: int = 0
    errors: int = 0
    closed: bool = False
    last_tick: int = 0
    villagers: int = 0
    army: int = 0
    buildings: int = 0
    population: int = 0
    age: str = "DARK"
    accepted: int = 0
    rejected: int = 0
    stats: dict[str, int] = field(default_factory=dict)


class Host:
    """Connects a brain to every token and runs them until a stop condition."""

    def __init__(
        self,
        url: str,
        tokens: Sequence[str],
        config: Optional[Config] = None,
        brain_factory: Optional[Callable[[int], Brain]] = None,
    ) -> None:
        self.config = config or Config()
        make = brain_factory or (lambda slot: Brain(self.config, name=f"bot{slot}"))
        self.url = url
        self.brains = [make(index) for index in range(len(tokens))]
        self.clients = [
            aoeworld.connect(url, token, decision_hz=self.config.decision_hz) for token in tokens
        ]
        self.reports = [BotReport(slot=index) for index in range(len(tokens))]
        self.open = set(range(len(tokens)))

    # ----- the loop --------------------------------------------------------
    def _handle(self, index: int, decision: Optional[aoeworld.Decision]) -> None:
        report = self.reports[index]
        if decision is None:
            report.closed = True
            self.open.discard(index)
            return
        try:
            self.brains[index].decide(self.clients[index], decision)
        except Exception:  # one bot's bug must not stop the others
            report.errors += 1
            log.exception("bot %d failed at tick %d", index, decision.tick)
        report.decisions += 1
        report.last_tick = decision.tick
        self._summarise(index, decision.world)

    def _summarise(self, index: int, world: aoeworld.World) -> None:
        report, brain = self.reports[index], self.brains[index]
        report.player = world.me
        units = world.my_units()
        report.villagers = sum(1 for u in units if u.kind == aoeworld.UnitKind.VILLAGER)
        report.army = sum(1 for u in units if u.military)
        report.buildings = len(world.my_buildings())
        economy = world.economy
        if economy is not None:
            report.population = economy.population
            report.age = str(economy.age).split(".")[-1]
        report.accepted = brain.memory.stats.get("accepted", 0)
        report.rejected = brain.memory.stats.get("rejected", 0)
        report.stats = dict(brain.memory.stats)

    def run(
        self,
        until_tick: Optional[int] = None,
        max_seconds: Optional[float] = None,
        progress: Optional[Callable[[list[BotReport]], None]] = None,
        progress_every: float = 10.0,
    ) -> list[BotReport]:
        """Drive every bot until all reach `until_tick`, close, or time runs out."""
        fleet = aoeworld.Fleet(self.clients)
        started = last_report = time.monotonic()
        try:
            while self.open:
                for index, decision in fleet.wait(timeout=1.0):
                    if index in self.open:
                        self._handle(index, decision)
                now = time.monotonic()
                if progress and now - last_report >= progress_every:
                    progress(self.reports)
                    last_report = now
                if max_seconds is not None and now - started > max_seconds:
                    break
                if until_tick is not None and all(
                    self.reports[i].last_tick >= until_tick for i in self.open
                ):
                    break
        finally:
            self.close()
        return self.reports

    async def run_async(self, until_tick: Optional[int] = None, max_seconds: Optional[float] = None) -> list[BotReport]:
        """The same loop for an asyncio program; the host's thread stays free."""
        fleet = aoeworld.AsyncFleet(aoeworld.Fleet(self.clients))
        started = time.monotonic()
        try:
            while self.open:
                for index, decision in await fleet.wait(1.0):
                    if index in self.open:
                        self._handle(index, decision)
                if max_seconds is not None and time.monotonic() - started > max_seconds:
                    break
                if until_tick is not None and all(
                    self.reports[i].last_tick >= until_tick for i in self.open
                ):
                    break
                await asyncio.sleep(0)
        finally:
            fleet.close()
            self.close()
        return self.reports

    def close(self) -> None:
        for client in self.clients:
            client.close()


def start_and_run(
    url: str,
    admin_token: str,
    bots: int,
    humans: int = 0,
    minutes: float = 5.0,
    tiles_per_side: int = 500,
    seed: int = 1,
    config: Optional[Config] = None,
) -> list[BotReport]:
    """Start a match (humans first, then AI slots) and run every AI in it."""
    slots = ["human"] * humans + ["ai"] * bots
    info = aoeworld.start_match(url, admin_token, slots, tiles_per_side=tiles_per_side, seed=seed)
    tokens = [slot.token for slot in info.slots if slot.kind == "ai"]
    host = Host(url, tokens, config)
    ticks = int(minutes * 60 * 20)
    return host.run(until_tick=ticks)
