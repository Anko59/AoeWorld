"""The AI arena: bots play a match in lockstep, as fast as the machine allows.

A server started with ``AOE_ARENA=1`` has no tick clock; this driver advances it
with ``POST /match/step``. The bots connect through the same protocol and SDK as
in a real match and see the same fog-filtered view, and they keep their
``decision_hz`` in *game* time: the driver steps one decision period, waits until
every bot has its decision, lets them all decide, then steps again. Orders sent
at tick *t* are applied on the first tick after it. A seed fixes terrain and
village placement; view-delivery timing can still change decisions (see
``docs/ai-sdk.md``). Wall time depends on armies and machine load.
"""

from __future__ import annotations

import http.client
import json
import os
import socket
import subprocess
import sys
import tempfile
import time
import urllib.parse
from pathlib import Path
from typing import Callable, Optional

import aoeworld

from .config import Config
from .host import BotReport, Host
from .diagnostics import outcomes, trace

TICK_HZ = 20
SECRET = os.environ.get("AOE_MATCH_TOKEN") or "arena-match-secret-0123456789"
#: Wall seconds to let a step's messages reach the bots before they are polled.
SETTLE_SECONDS = 0.004
#: Wall seconds a missing decision is waited for before one more tick is stepped.
NUDGE_SECONDS = 1.0
MAX_NUDGES_TICK = 10**7


class Stepper:
    """The server's step endpoint over one persistent connection."""

    def __init__(self, url: str) -> None:
        parts = urllib.parse.urlsplit(url)
        self.connection = http.client.HTTPConnection(parts.hostname, parts.port, timeout=120)

    def step(self, ticks: int, orders: int) -> int:
        body = json.dumps({"ticks": ticks, "orders": orders})
        self.connection.request(
            "POST", "/match/step", body, {"x-aoeworld-admin-token": SECRET, "content-type": "application/json"}
        )
        response = self.connection.getresponse()
        text = response.read().decode()
        if response.status != 200:
            raise RuntimeError(f"step refused: {response.status} {text}")
        return json.loads(text)["tick"]


def free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


class ArenaServer:
    """A throw-away `aoe-server` in arena mode."""

    def __init__(self, binary: Path, root: Path) -> None:
        self.binary, self.root = binary, root
        self.port = free_port()
        self.url = f"http://127.0.0.1:{self.port}"
        self.packages = tempfile.TemporaryDirectory(prefix="arena-maps-")

    def __enter__(self) -> "ArenaServer":
        env = dict(
            os.environ,
            AOE_BIND=f"127.0.0.1:{self.port}",
            AOE_MATCH_TOKEN=SECRET,
            AOE_TICK_HZ=str(TICK_HZ),
            AOE_ARENA="1",
            AOE_MAP_PACKAGE_DIRECTORY=self.packages.name,
        )
        self.process = subprocess.Popen(
            [str(self.binary)], cwd=self.root, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        )
        deadline = time.time() + 30
        while time.time() < deadline:
            try:
                http.client.HTTPConnection("127.0.0.1", self.port, timeout=1).request("GET", "/health")
                return self
            except OSError:
                if self.process.poll() is not None:
                    raise RuntimeError("aoe-server exited during startup")
                time.sleep(0.2)
        raise RuntimeError("aoe-server did not become healthy")

    def __exit__(self, *exc: object) -> None:
        self.process.terminate()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
        self.packages.cleanup()


def _await_decisions(fleet: "aoeworld.Fleet", open_bots: set[int], stepper: Stepper, tick: int) -> tuple[dict, int]:
    """Every open bot's decision, and the tick they were taken at.

    A bot whose client changed its map window during the step gets no message
    for the tick (the server drops updates of the old window), so a decision
    that does not come within a moment is helped along by one more tick."""
    got: dict[int, Optional[aoeworld.Decision]] = {}
    quiet = time.monotonic()
    while len(got) < len(open_bots):
        for index, decision in fleet.wait(timeout=0.05):
            got[index] = decision
        if time.monotonic() - quiet > NUDGE_SECONDS and len(got) < len(open_bots):
            tick = stepper.step(1, 0)
            quiet = time.monotonic()
            if tick > MAX_NUDGES_TICK:
                raise RuntimeError("bots made no decision")
    return got, tick


def play(
    url: str,
    bots: int,
    minutes: float,
    size: int,
    seed: int,
    config: Optional[Config] = None,
    stop: Optional[Callable[[list[BotReport]], bool]] = None,
    before_step: Optional[Callable[[int, list[BotReport]], None]] = None,
) -> dict:
    """One seeded match against a running arena server; the report is JSON-able."""
    config = config or Config()
    started = time.monotonic()
    info = aoeworld.start_match(url, SECRET, ["ai"] * bots, tiles_per_side=size, seed=seed)
    host = Host(url, [slot.token for slot in info.slots], config, orders_per_second=10**6)
    period = max(1, round(TICK_HZ / config.decision_hz))
    final = int(minutes * 60 * TICK_HZ)
    stepper, fleet = Stepper(url), aoeworld.Fleet(host.clients)
    sent = [0] * bots
    tick = stepper.step(period, 0)
    last_minute = -1
    try:
        while True:
            time.sleep(SETTLE_SECONDS)
            decisions, tick = _await_decisions(fleet, host.open, stepper, tick)
            for index, decision in sorted(decisions.items()):
                outcomes(host, index, decision, seed)
                host._handle(index, decision)
            if tick // (TICK_HZ * 60) != last_minute:
                last_minute = tick // (TICK_HZ * 60)
                trace(host, tick, seed)
            if os.environ.get("AOE_ARENA_TRACE") and tick % (TICK_HZ * 60) == 0:
                for r in host.reports:
                    eco = host.clients[r.slot].world.economy
                    print(
                        f"min {tick // 1200} bot{r.slot} {r.age} vill {r.villagers} army {r.army} pop {r.population} "
                        f"bld {r.buildings} food {eco.food} wood {eco.wood} gold {eco.gold}",
                        file=sys.stderr,
                        flush=True,
                    )
            orders = 0
            for index, client in enumerate(host.clients):
                orders += client.orders_sent - sent[index]
                sent[index] = client.orders_sent
            if tick >= final or not host.open or (stop is not None and stop(host.reports)):
                break
            if before_step is not None:
                before_step(tick, host.reports)  # a showcase paces or pauses the game here
            tick = stepper.step(period, orders)
    finally:
        host.close()
    return summary(host.reports, tick, seed, size, time.monotonic() - started)


def summary(reports: list[BotReport], tick: int, seed: int, size: int, wall: float) -> dict:
    """Per bot: Feudal time, villagers, army peak, attacks landed, losses, kills."""
    per_bot = []
    for r in reports:
        feudal = r.stats.get("feudal_tick")
        castle = r.stats.get("castle_tick")
        per_bot.append(
            {
                "slot": r.slot,
                "feudal_minute": round(feudal / (TICK_HZ * 60), 2) if feudal else None,
                "castle_minute": round(castle / (TICK_HZ * 60), 2) if castle else None,
                "castle_units_peak": r.stats.get("castle_units_peak", 0),
                "villagers": r.villagers,
                "villagers_peak": r.stats.get("villagers_peak", 0),
                "army": r.army,
                "army_peak": r.stats.get("army_peak", 0),
                "attacks_launched": r.stats.get("attacks_launched", 0),
                "attacks_landed": r.stats.get("attack_hits", 0),
                "units_lost": r.stats.get("units_lost", 0),
                "kills": r.stats.get("kills", 0),
                "buildings": r.buildings,
                "errors": r.errors,
            }
        )
    return {
        "seed": seed,
        "map_tiles": size,
        "game_minutes": round(tick / (TICK_HZ * 60), 2),
        "wall_seconds": round(wall, 1),
        "bots": per_bot,
    }


def proven(reports: list[BotReport], soldiers: int) -> bool:
    """Every bot at least in Feudal Age with `soldiers` at once, and a blow landed."""
    return all(
        r.age in ("FEUDAL", "CASTLE") and r.stats.get("army_peak", 0) >= soldiers for r in reports
    ) and any(r.stats.get("attack_hits", 0) > 0 for r in reports)


def run_seeds(binary: Path, root: Path, seeds: list[int], bots: int, minutes: float, size: int,
              soldiers: Optional[int] = None, config: Optional[Config] = None) -> list[dict]:
    """A fresh server and match per seed. `soldiers` stops a seed once proven."""
    results = []
    for seed in seeds:
        with ArenaServer(binary, root) as server:
            stop = (lambda reports: proven(reports, soldiers)) if soldiers else None
            results.append(play(server.url, bots, minutes, size, seed, config, stop))
    return results
