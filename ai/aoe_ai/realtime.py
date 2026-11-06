"""Real-time run of one draft: a release server with its own tick clock, N bots of
one draft, the server's tick rate and tick cost sampled while they play.

``python -m aoe_ai realtime --server-bin B --draft D --bots 6 --seconds 480``.
The arena has no tick clock, so this is where a draft's effect on the 20 Hz
budget shows (docs/scale/match.md). Optional ``--villagers`` seeds extra start
units so the management stage comes sooner.
"""

from __future__ import annotations

import http.client
import json
import os
import subprocess
import threading
import time
from pathlib import Path
from typing import Optional

from .arena import SECRET, free_port
from .drafts import draft
from .host import Host
from .tournament import admin_match


def _call(port: int, method: str, path: str, body: Optional[dict] = None) -> dict:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=60)
    headers = {"x-aoeworld-admin-token": SECRET, "content-type": "application/json"}
    connection.request(method, path, json.dumps(body) if body is not None else None, headers)
    return json.loads(connection.getresponse().read().decode())


def run(binary: Path, root: Path, name: str, bots: int, seconds: float, map_hash: str,
        packages: str, villagers: int = 0, seed: int = 1) -> dict:
    port = free_port()
    env = dict(os.environ, AOE_BIND=f"127.0.0.1:{port}", AOE_MATCH_TOKEN=SECRET, AOE_MAP_PACKAGE_DIRECTORY=packages)
    server = subprocess.Popen([str(binary)], cwd=root, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            try:
                http.client.HTTPConnection("127.0.0.1", port, timeout=1).request("GET", "/health")
                break
            except OSError:
                time.sleep(0.2)
        request = {"map_hash": map_hash, "slots": ["ai"] * bots, "seed": seed}
        if villagers:
            request["start_units"] = {"villagers": villagers, "militia": 0}
        info = _call(port, "POST", "/match", request)
        tokens = [s["token"] for s in info["slots"] if s["kind"] == "ai"]
        host = Host(f"http://127.0.0.1:{port}", tokens, draft(name))
        done = threading.Event()
        samples: list[dict] = []

        def watch() -> None:
            while not done.wait(15):
                perf = _call(port, "GET", "/admin/perf")
                samples.append({"tick_rate": perf.get("tick_rate"), "missed_ticks": perf.get("missed_ticks"),
                                "missed_total": perf.get("missed_total")})

        threading.Thread(target=watch, daemon=True).start()
        started = time.monotonic()
        host.run(max_seconds=seconds)
        done.set()
        wall = time.monotonic() - started
        page = _call(port, "GET", "/match/stats?since=0")
        records = page["ticks"]
        total = sorted(r[1] for r in records) or [0]
        pick = lambda q: total[min(len(total) - 1, int(q * len(total)))]
        final = admin_match(port)
        rates = [s["tick_rate"] for s in samples if s["tick_rate"]]
        return {"draft": name, "bots": bots, "wall_seconds": round(wall), "game_tick": final["tick"],
                "ticks_per_second": round(final["tick"] / wall, 2), "tick_rate_samples_min": min(rates, default=None),
                "tick_p50_ms": pick(0.5) / 1000, "tick_p95_ms": pick(0.95) / 1000, "tick_p99_ms": pick(0.99) / 1000,
                "missed_total": page.get("missed_total"), "orders_total": sum(r[4] for r in records),
                "ages": [p["age"] for p in final["players"]], "population": [p["population"] for p in final["players"]],
                "managers": [{m: p["units"].get(m, 0) for m in ("Steward", "Vassal", "General")} for p in final["players"]]}
    finally:
        server.terminate()
        try:
            server.wait(timeout=10)
        except subprocess.TimeoutExpired:
            server.kill()
