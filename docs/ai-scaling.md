# Long-run AI scaling benchmark

The scaling workload runs the ordinary Python Brain through the public SDK on
an unmodified source-backed France or Mekong package. Measurement uses the
admin API for authoritative kills and the bot's own economy and buildings for
its development. Admin observations are never passed into a Brain.

Required samples are 30, 60 and 120 game minutes, including eliminated slots.
A missing checkpoint/timing record, bot error, lost connection or timeout makes the
run incomplete; a partial game is never a completed baseline. The seed, package
hash, start anchors, actual starts, configuration and source revision identify
the workload. Network delivery remains a source of variation in the lockstep
arena, so comparisons require the same inputs and should report that limit.

For every slot and checkpoint record population, completed Town Centers, age,
researched technologies, banked food/wood/gold/stone, military units and kills.
`settled_cells` is a development footprint: the number of distinct 64-by-64 tile
cells containing a completed Town Center, Castle or military production
building. It measures spatial expansion, not exclusive ownership of terrain.
Record the building positions as well so the footprint is auditable.

Hard starts use explicit local anchors in `POST /match`. These retain the
normal village kit and bounded local village search, but omit the main-continent
and distant fallback searches. The response records actual scout positions and
the coarse hydrology main-land classification. A forest start must additionally
have at least 200 natural trees within 32 tiles of its starting village; the twelve kit trees
alone do not establish a forest start. A package name alone proves neither a
forest nor an island start.

Record tick cost distributions over the full run, per-bot CPU decision cost,
order totals and maximum orders per decision. Lockstep wall speed is diagnostic;
it does not establish the live server's tick rate or qualify dedicated hardware.
A p95 tick over 50 ms is outside the 20 Hz tick budget on the measured host.

Keep full JSON measurements, map packages, geodata and recordings outside Git.
Only the reproducible workload definition and measured summary belong here.

## Reproduce

The checked-in [France workload](../qa/ai-scaling.json) fixes four starts:

| slot | case | anchor | actual scout start | natural trees within 32 tiles |
| --- | --- | --- | --- | --- |
| 0 | island (off the main land mass) | 66, 51 | 63, 48 | 241 |
| 1 | forest | 1200, 600 | 1200, 600 | 576 |
| 2 | inland forest | 1000, 1200 | 1000, 1200 | 752 |
| 3 | open land | 700, 850 | 700, 850 | 0 |

Use the ignored map packages/geodata from the owner's play13 checkout, then run:

```sh
make ai-benchmark AI_BENCHMARK_ARGS='--map-hash dc7e0d4b253af4e7e7106b69eba61169ef2f8f6b0fd7781fbe5a3ed6ccc60e68 --starts qa/ai-scaling.json --out .cache/ai-scaling/baseline.json'
```

`--check-starts` validates the start cases without advancing the game. Output
paths must be fresh. The result JSON retains incomplete status until all three
checkpoints arrive. The adjacent `.ticks.jsonl` contains the full timing stream.
The server and SDK wheel use release builds; the benchmark runs one ordinary
Brain per slot, at the default two decisions per game second.

During a live run, `.cache/ai-scaling/live.json` identifies its local server.
`/pkg/ai-view.html` opens slot 1 through the ordinary browser slot token for
filming with `make showcase`. This ignored temporary bootstrap contains a local
slot credential and is removed on exit. It sends no orders and keeps fog on.
Keep the recording outside Git and attach it with `SHIP_VIDEO` when shipping.

## Recorded baseline

Measured 2026-11-13 from AI revision `a87da7b00b7ac2ae5edd62e13beaae0edd19c6e6`,
with the measurement instrumentation introduced here. The unchanged Brain
and configuration are the comparison point for subsequent behavior changes.
Full evidence is `.cache/ai-scaling/baseline-recorded.json` and its adjacent
`.ticks.jsonl`; source SHA-256 values are embedded in the JSON.

| minute | slot | pop | TCs | settled cells | age | techs | food / wood / gold / stone | military | kills |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 30 | 0 | 99 | 1 | 2 | Castle | 5 | 2 / 20 / 46 / 200 | 61 | 0 |
| 30 | 1 | 67 | 1 | 1 | Castle | 5 | 24 / 75 / 210 / 200 | 36 | 0 |
| 30 | 2 | 47 | 1 | 2 | Feudal | 3 | 1850 / 3771 / 115 / 200 | 16 | 0 |
| 30 | 3 | 38 | 1 | 2 | Feudal | 3 | 2920 / 12 / 395 / 200 | 7 | 0 |
| 60 | 0 | 295 | 1 | 2 | Castle | 8 | 435 / 78 / 37 / 200 | 233 | 0 |
| 60 | 1 | 193 | 1 | 2 | Castle | 5 | 19 / 55 / 40 / 200 | 161 | 0 |
| 60 | 2 | 46 | 1 | 2 | Feudal | 3 | 5875 / 15698 / 115 / 200 | 16 | 0 |
| 60 | 3 | 117 | 1 | 2 | Castle | 5 | 1175 / 7 / 3729 / 200 | 55 | 1 |
| 120 | 0 | 295 | 1 | 2 | Castle | 10 | 10243 / 16457 / 37 / 200 | 233 | 0 |
| 120 | 1 | 365 | 1 | 2 | Castle | 10 | 6333 / 10346 / 43 / 200 | 316 | 0 |
| 120 | 2 | 34 | 1 | 2 | Feudal | 3 | 15980 / 38178 / 115 / 200 | 4 | 20 |
| 120 | 3 | 289 | 1 | 2 | Imperial | 8 | 13 / 2435 / 899 / 200 | 229 | 13 |

| minute | median pop | median TCs | median age | median techs |
| --- | --- | --- | --- | --- |
| 30 | 57 | 1 | 1.5 | 4 |
| 60 | 155 | 1 | 2 | 5 |
| 120 | 292 | 1 | 2 | 9 |

Age medians use Dark=0, Feudal=1, Castle=2, Imperial=3.

Run diagnostics:

```json
{
  "ticks": {
    "count": 144009,
    "tick_p50_us": 5992,
    "tick_p95_us": 16036,
    "tick_max_us": 1413698,
    "orders": 24108
  },
  "wall_seconds": 3006.180930429022
}
```

| slot | CPU ms/decision | slowest CPU ms | slowest wall ms | max orders/decision | orders |
| --- | --- | --- | --- | --- | --- |
| 0 | 40.019 | 688.175 | 841.302 | 14 | 5193 |
| 1 | 32.476 | 337.255 | 337.721 | 32 | 10116 |
| 2 | 9.098 | 189.154 | 278.324 | 7 | 2399 |
| 3 | 27.360 | 200.237 | 200.247 | 18 | 6405 |

This is one four-player run on a shared host, with actual game footage recorded
during it. It establishes a reproducible behavior baseline; it does not
establish 100-player performance or dedicated hardware qualification.
The earlier `.cache/ai-scaling/baseline.json` attempt was stopped after
detecting absent arena tick records; it is incomplete and is excluded.
