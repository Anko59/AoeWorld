"""The Python AI against the Rust AI: the same seeded arena match played once by
each, measured the same way from the server's admin summary.

The server plays Rust seats itself (``AOE_AI_RUNTIME=rust``), so in that run the
driver only steps the clock; in the Python run it also hosts the bots, as
``make ai-arena`` does. Every ``SAMPLE_TICKS`` both runs read ``/admin/match``:

- Feudal and Castle time: the first sample at that age;
- villagers and army (every unit but those in ``NOT_ARMY``: workers, the
  scout, traders and management units) at each minute of ``CHECKPOINTS``;
- first attack: the first sample where at least ``ATTACK_SOLDIERS`` soldiers
  have their centre more than ``ATTACK_TILES`` from the first Town Center.

Bots of one run are averaged. The gap is Rust minus Python, checked against
``TOLERANCE``; a run where only one side attacks counts as out of tolerance.
"""

from __future__ import annotations

import json
import math
import time
import urllib.request
from pathlib import Path
from typing import Optional

import aoeworld

from .arena import SECRET, TICK_HZ, ArenaServer, Stepper, play
from .config import Config
from .tournament import CostBrain

SAMPLE_TICKS = 200
ATTACK_TILES = 40.0
ATTACK_SOLDIERS = 3
NOT_ARMY = frozenset({"Villager", "FishingShip", "Scout", "Steward", "Vassal", "General", "TradeCart", "TradeCog"})
CHECKPOINTS = (5, 10, 15, 20)
#: Minutes for the ages and the first attack; for counts, a share of the
#: Python value and at least two units.
TOLERANCE = {"feudal_minute": 1.5, "castle_minute": 2.0, "first_attack_minute": 3.0,
             "villagers": 0.20, "army": 0.30}


def admin(url: str, path: str) -> dict:
    request = urllib.request.Request(url + path, headers={"x-aoeworld-admin-token": SECRET})
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.loads(response.read())


class Samples:
    """Per AI player: the first minute at each age, counts at the checkpoints and
    the first attack, from admin samples taken in tick order."""

    def __init__(self) -> None:
        self.players: dict[int, dict] = {}

    def take(self, url: str) -> None:
        state = admin(url, "/admin/match")
        minute = state["tick"] / (TICK_HZ * 60)
        for player in state["players"]:
            if player["kind"] != "ai":
                continue
            seen = self.players.setdefault(player["id"], {"home": None, "counts": {}})
            if seen["home"] is None and player["town_centers"]:
                seen["home"] = player["town_centers"][0]
            age = player["age"]
            if age in ("Feudal", "Castle", "Imperial"):
                seen.setdefault("feudal_minute", round(minute, 2))
            if age in ("Castle", "Imperial"):
                seen.setdefault("castle_minute", round(minute, 2))
            units = player["units"]
            villagers = units.get("Villager", 0)
            soldiers = self.soldiers(url, player["id"])
            for checkpoint in CHECKPOINTS:
                if minute >= checkpoint and checkpoint not in seen["counts"]:
                    seen["counts"][checkpoint] = {"villagers": villagers, "army": len(soldiers)}
                    seen.setdefault("detail", {})[checkpoint] = {
                        "buildings": player["buildings"], "resources": player["resources"], "units": units}
            home = seen["home"]
            if home and len(soldiers) >= ATTACK_SOLDIERS:
                centre = (sum(x for x, _ in soldiers) / len(soldiers), sum(y for _, y in soldiers) / len(soldiers))
                if math.dist(centre, (home["x"], home["y"])) > ATTACK_TILES:
                    seen.setdefault("first_attack_minute", round(minute, 2))

    @staticmethod
    def soldiers(url: str, player: int) -> list[tuple[float, float]]:
        """Positions of the player's soldiers: every unit but workers, scouts,
        ships' fishermen, traders and management units."""
        found, offset = [], 0
        while offset is not None:
            page = admin(url, f"/admin/match/players/{player}?offset={offset}&limit=100")
            for entity in page["entities"]:
                if entity["category"] == "unit" and entity["kind"] not in NOT_ARMY:
                    found.append((entity["position"]["x"], entity["position"]["y"]))
            offset = page["next_offset"]
        return found

    def summary(self) -> dict:
        players = list(self.players.values())

        def mean(values):
            values = [v for v in values if v is not None]
            return round(sum(values) / len(values), 2) if values else None

        out = {key: mean(p.get(key) for p in players)
               for key in ("feudal_minute", "castle_minute", "first_attack_minute")}
        out["attackers"] = sum("first_attack_minute" in p for p in players)
        out["detail"] = [p.get("detail", {}) for p in players]
        for checkpoint in CHECKPOINTS:
            for count in ("villagers", "army"):
                out[f"{count}_{checkpoint}"] = mean(p["counts"].get(checkpoint, {}).get(count) for p in players)
        return out


def run_python(binary: Path, root: Path, seed: int, bots: int, minutes: float, size: int) -> dict:
    samples, brains = Samples(), []
    last = [-1]

    def factory(slot):
        brains.append(CostBrain(Config(), f"bot{slot}"))
        return brains[-1]

    def observe(tick, host, info):
        if tick // SAMPLE_TICKS != last[0]:
            last[0] = tick // SAMPLE_TICKS
            samples.take(server.url)

    with ArenaServer(binary, root, ai="python") as server:
        result = play(server.url, bots, minutes, size, seed, brain_factory=factory, observe=observe)
    out = samples.summary()
    calls = sum(b.calls for b in brains)
    out["cpu_ms_per_decision"] = round(1000 * sum(b.cpu for b in brains) / calls, 3) if calls else None
    out["decisions"] = calls
    out["counters"] = [dict(b.board.stats) for b in brains]
    out["wall_seconds"] = result["wall_seconds"]
    return out


def run_rust(binary: Path, root: Path, seed: int, bots: int, minutes: float, size: int) -> dict:
    samples = Samples()
    started = time.monotonic()
    with ArenaServer(binary, root, ai="rust") as server:
        aoeworld.start_match(server.url, SECRET, ["ai"] * bots, tiles_per_side=size, seed=seed)
        stepper, final, tick = Stepper(server.url), int(minutes * 60 * TICK_HZ), 0
        while tick < final:
            tick = stepper.step(SAMPLE_TICKS, 0)
            samples.take(server.url)
        cost = admin(server.url, "/admin/perf").get("rust_ai", {})
    out = samples.summary()
    decisions = cost.get("decisions", 0)
    out["cpu_ms_per_decision"] = round(cost.get("cpu_ms", 0) / decisions, 3) if decisions else None
    out["decisions"] = decisions
    out["tick_thread"] = cost.get("tick_thread")
    out["counters"] = cost.get("seats")
    out["wall_seconds"] = round(time.monotonic() - started, 1)
    return out


def gaps(python: dict, rust: dict) -> dict:
    """Rust minus Python per measure, and whether each is within ``TOLERANCE``."""
    out = {}
    for key, value in python.items():
        if key in ("cpu_ms_per_decision", "decisions", "wall_seconds", "attackers", "tick_thread", "detail", "counters"):
            continue
        other = rust.get(key)
        measure = key.rsplit("_", 1)[0] if key.split("_")[-1].isdigit() else key
        if value is None or other is None:
            out[key] = {"python": value, "rust": other, "within": value is None and other is None}
            continue
        gap = round(other - value, 2)
        # Counts: a share of the Python value, and never less than two units.
        limit = max(TOLERANCE[measure] * value, 2) if measure in ("villagers", "army") else TOLERANCE[measure]
        out[key] = {"python": value, "rust": other, "gap": gap, "within": abs(gap) <= limit}
    return out


def compare(binary: Path, root: Path, seeds: list[int], bots: int, minutes: float, size: int,
            only: Optional[str] = None) -> dict:
    runs = []
    for seed in seeds:
        python = run_python(binary, root, seed, bots, minutes, size) if only != "rust" else {}
        rust = run_rust(binary, root, seed, bots, minutes, size) if only != "python" else {}
        runs.append({"seed": seed, "python": python, "rust": rust,
                     "gaps": gaps(python, rust) if python and rust else {}})
    within = all(g["within"] for run in runs for g in run["gaps"].values())
    return {"bots": bots, "minutes": minutes, "tiles": size, "tolerance": TOLERANCE,
            "sample_ticks": SAMPLE_TICKS, "attack_tiles": ATTACK_TILES, "attack_soldiers": ATTACK_SOLDIERS,
        "runs": runs, "within": within}
