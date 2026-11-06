"""Free-for-all of drafts on the lockstep arena, scored from the admin API.

``python -m aoe_ai tournament --server-bin B --drafts a,b,c --seeds 1,2``: one
fresh server and match per seed, the drafts rotated over the slots so no draft
keeps the best village. Every game minute the admin ``/admin/match`` view is
sampled into ``.cache/ai5/games``; the result is JSON per game plus a ranking.
"""

from __future__ import annotations

import http.client
import json
import time
from pathlib import Path
from typing import Optional

from . import arena
from .brain import Brain
from .drafts import DRAFTS, draft

AGE_RANK = {"Dark": 0, "Feudal": 1, "Castle": 2, "Imperial": 3}
MANAGERS = ("Steward", "Vassal", "General")


def admin_match(port: int) -> dict:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=60)
    connection.request("GET", "/admin/match", headers={"x-aoeworld-admin-token": arena.SECRET})
    return json.loads(connection.getresponse().read().decode())


def admin_perf(port: int) -> dict:
    """The server's tick records (lockstep: sim cost per tick, not wall rate)."""
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=60)
    connection.request("GET", "/admin/perf", headers={"x-aoeworld-admin-token": arena.SECRET})
    document = json.loads(connection.getresponse().read().decode())
    return {k: v for k, v in document.items() if k not in ("gameplay_memory", "terrain_cache", "snapshot_store")}


class CostBrain(Brain):
    """A brain that counts its own CPU: thread time inside `decide` (the host runs
    every bot on one thread, so this is that bot's whole AI cost) and its orders."""

    def __init__(self, config, name):
        super().__init__(config, name)
        self.cpu = 0.0
        self.slowest = 0.0
        self.calls = 0
        self.sent = 0

    def decide(self, client, decision):
        begin = time.thread_time()
        try:
            super().decide(client, decision)
        finally:
            spent = time.thread_time() - begin
            self.cpu += spent
            self.slowest = max(self.slowest, spent)
            self.calls += 1
            self.sent = client.orders_sent


class Tracker:
    """Per player: age arrival minutes, peaks, and the last snapshot."""

    def __init__(self, names: list[str]) -> None:
        self.names = names
        self.rows = [dict(draft=n, slot=i, age_minute={}, pop_peak=0, villagers_peak=0, army_peak=0,
                          managers_peak={m: 0 for m in MANAGERS}, buildings_peak=0) for i, n in enumerate(names)]
        self.last: dict = {}

    def sample(self, snapshot: dict, minute: float) -> None:
        self.last = snapshot
        for player in snapshot["players"]:
            row = self.rows[player["id"]]
            age = player["age"]
            for name, rank in AGE_RANK.items():
                if rank <= AGE_RANK.get(age, 0) and name not in row["age_minute"]:
                    row["age_minute"][name] = round(minute, 2)
            units = player.get("units", {})
            row["pop_peak"] = max(row["pop_peak"], player["population"])
            row["villagers_peak"] = max(row["villagers_peak"], units.get("Villager", 0))
            row["army_peak"] = max(row["army_peak"], player["army"]["size"])
            for m in MANAGERS:
                row["managers_peak"][m] = max(row["managers_peak"][m], units.get(m, 0))
            row["buildings_peak"] = max(row["buildings_peak"], sum(player["buildings"].values()))

    def finish(self) -> list[dict]:
        for player in self.last.get("players", []):
            row = self.rows[player["id"]]
            units = player.get("units", {})
            row.update(age=player["age"], population=player["population"], villagers=units.get("Villager", 0),
                       army=player["army"]["size"], kills=player["kills"], losses=player["losses"],
                       status=player["status"], buildings=sum(player["buildings"].values()),
                       castles=player["buildings"].get("Castle", 0), town_centers=player["buildings"].get("TownCenter", 0),
                       managers_end={m: units.get(m, 0) for m in MANAGERS})
            row["score"] = score(row)
        return self.rows


def score(row: dict) -> int:
    """Heuristic ranking: age first, then people and net kills; survival bonus."""
    return (200 * AGE_RANK.get(row["age"], 0) + row["population"] + 2 * row["kills"] - row["losses"]
            + (150 if row["status"] == "alive" else 0))


#: Cost limits: a draft past either is not acceptable however strong it is.
ORDERS_PER_MIN_LIMIT = 600
CPU_MS_PER_DECISION_LIMIT = 25.0


def adjusted(row: dict) -> float:
    """Strength minus cost: 1 point per order/min over half the limit, 4 per
    CPU ms/decision over half the limit, and a flat -300 past either limit."""
    penalty = max(0.0, row["orders_per_min"] - ORDERS_PER_MIN_LIMIT / 2) + 4 * max(0.0, row["cpu_ms_per_decision"] - CPU_MS_PER_DECISION_LIMIT / 2)
    if row["orders_per_min"] > ORDERS_PER_MIN_LIMIT or row["cpu_ms_per_decision"] > CPU_MS_PER_DECISION_LIMIT:
        penalty += 300
    return row["score"] - penalty


def play_game(binary: Path, root: Path, names: list[str], seed: int, size: int, minutes: float,
              wall_limit: float, out: Path, map_hash: Optional[str] = None) -> dict:
    """One free-for-all. `names` are the drafts by slot."""
    tracker = Tracker(names)
    brains: dict = {}
    perf: list = []
    started = time.monotonic()
    with arena.ArenaServer(binary, root) as server:
        samples = out.with_suffix(".jsonl").open("w", encoding="utf-8")
        seen = [-1]

        def before_step(tick: int, _reports: list) -> None:
            minute = tick / (arena.TICK_HZ * 60)
            if int(minute) != seen[0]:
                seen[0] = int(minute)
                snapshot = admin_match(server.port)
                tracker.sample(snapshot, minute)
                perf.append(admin_perf(server.port))
                samples.write(json.dumps({"minute": int(minute), "players": snapshot["players"]}) + "\n")
                samples.flush()

        def stop(_reports: list) -> bool:
            alive = [p for p in tracker.last.get("players", []) if p["status"] == "alive"]
            return time.monotonic() - started > wall_limit or (bool(tracker.last) and len(alive) <= 1)

        result = arena.play(server.url, len(names), minutes, size, seed, Config_for(names[0]), stop=stop,
                            before_step=before_step, brain_factory=lambda i: brains.setdefault(i, CostBrain(draft(names[i]), f"{names[i]}{i}")),
                            map_hash=map_hash)
        tracker.sample(admin_match(server.port), result["game_minutes"])
        samples.close()
    rows = tracker.finish()
    minutes_played = max(result["game_minutes"], 0.01)
    for row in rows:
        brain = brains[row["slot"]]
        row.update(orders=brain.sent, orders_per_min=round(brain.sent / minutes_played, 1),
                   cpu_seconds=round(brain.cpu, 2), cpu_ms_per_decision=round(1000 * brain.cpu / max(1, brain.calls), 2),
                   cpu_ms_slowest=round(1000 * brain.slowest, 1), decisions=brain.calls)
    for row, bot in zip(rows, result["bots"]):
        row["bot_errors"] = bot["errors"]
        row["attacks_launched"] = bot["attacks_launched"]
    return dict(perf=perf[-1] if perf else {}, seed=seed, size=size, map_hash=map_hash, game_minutes=result["game_minutes"],
                wall_seconds=result["wall_seconds"], players=rows)


def Config_for(name: str):
    return draft(name)


def rotate(names: list[str], seed: int) -> list[str]:
    k = seed % len(names)
    return names[k:] + names[:k]


def run(binary: Path, root: Path, drafts: list[str], seeds: list[int], size: int, minutes: float,
        wall_limit: float, label: str, map_hash: Optional[str] = None) -> list[dict]:
    out_dir = root / ".cache" / "ai5" / "games"
    out_dir.mkdir(parents=True, exist_ok=True)
    games = []
    for seed in seeds:
        names = rotate(drafts, seed)
        game = play_game(binary, root, names, seed, size, minutes, wall_limit, out_dir / f"{label}-seed{seed}",
                         map_hash)
        games.append(game)
        (out_dir / f"{label}-seed{seed}.json").write_text(json.dumps(game, indent=1))
        print(table(game), flush=True)
    return games


def table(game: dict) -> str:
    lines = [f"seed {game['seed']} size {game['size']} {game['game_minutes']} game min in {game['wall_seconds']} s",
             "draft            age       F/C/I min      pop vill army  kills lost  S/V/G  ord/min cpu ms/dec  score"]
    for p in sorted(game["players"], key=lambda r: -r.get("score", 0)):
        a = p["age_minute"]
        lines.append(
            f"{p['draft']:<16} {p.get('age', '?'):<9} {a.get('Feudal', '-')}/{a.get('Castle', '-')}/{a.get('Imperial', '-')}"
            f"  pk {p['pop_peak']:>4}/{p.get('population', 0):>4} {p['villagers_peak']:>4} {p['army_peak']:>4}"
            f"  {p.get('kills', 0):>4} {p.get('losses', 0):>4}  "
            f"{p['managers_peak']['Steward']}/{p['managers_peak']['Vassal']}/{p['managers_peak']['General']}  {p.get('orders_per_min', 0):>7} {p.get('cpu_ms_per_decision', 0):>9}  {p.get('score', 0)}")
    return "\n".join(lines)


def scoreboard(games: list[dict]) -> str:
    """Per draft over every game: mean score, wins and Imperial arrivals."""
    per: dict[str, list] = {}
    for game in games:
        best = max(p["score"] for p in game["players"])
        for p in game["players"]:
            per.setdefault(p["draft"], []).append((p, p["score"] == best))
    lines = ["| draft | games | wins | mean score | Imperial (games) | first Imperial min | mean peak pop | mean kills | mean losses | orders/min | CPU ms/decision | slowest decision ms | adjusted score |",
             "|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    for name, rows in sorted(per.items(), key=lambda kv: -sum(r["score"] for r, _ in kv[1]) / len(kv[1])):
        n = len(rows)
        imp = [r["age_minute"]["Imperial"] for r, _ in rows if "Imperial" in r["age_minute"]]
        lines.append(f"| {name} | {n} | {sum(w for _, w in rows)} | {sum(r['score'] for r, _ in rows) / n:.0f} | {len(imp)} | "
                     f"{min(imp) if imp else '-'} | {sum(r['pop_peak'] for r, _ in rows) / n:.0f} | "
                     f"{sum(r['kills'] for r, _ in rows) / n:.1f} | {sum(r['losses'] for r, _ in rows) / n:.1f} | "
                     f"{sum(r['orders_per_min'] for r, _ in rows) / n:.0f} | {sum(r['cpu_ms_per_decision'] for r, _ in rows) / n:.1f} | "
                     f"{max(r['cpu_ms_slowest'] for r, _ in rows):.0f} | {sum(adjusted(r) for r, _ in rows) / n:.0f} |")
    return "\n".join(lines)
