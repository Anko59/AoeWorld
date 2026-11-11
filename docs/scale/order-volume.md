# Owner-save movement order volume

The workload is a copy of `1794176817-first.save` (9,395 units, 10,581
buildings, 99 AI opponents), SHA-256
`c51582bf7eb4007ac5cdb72ef2ab3a733c7a451532b6b41d6816a2fc9c85c11a`.
The original is read-only. Inputs, full reports and recordings live under
ignored `.cache/order-volume/`; game assets and recordings are not Git inputs.

## Counter definitions

`GET /admin/match/routes` exposes cumulative `order_demand.issuers` for player,
Python AI, auto-scout, Steward, Vassal, General and rally/automation.
`decisions` counts source orders: every external command (including training,
research and settings), manager order attempts, and automatic trip starts.
Steward and General decisions use their existing order budgets; Vassal and
auto-scout decisions are counted at their issuing sites. Automatic trip starts
are unit-granular; formation commands from other sources count once.

The remaining counters count **unit movement intents**, including task movement and each member
of a formation, rather than protocol messages (one message can move many units).
Training, research, settings and repeated attacks that never request movement
are counted in `decisions`, but excluded from the trip counters. `scale-match` separately retains the server's
protocol command totals in its tick records.

- `orders`: movement requests, including redundant and refused requests once
  the unit and destination have been admitted to movement validation.
- `near_reissues`: requests within one tile (Euclidean distance, fixed-point)
  of the unit's current active goal, including a shared formation goal.
- `reassigned_10s`: accepted replacements within ten simulation seconds of
  that unit's previous accepted movement request, including completed trips.
- `superseded`: accepted movement requests or explicit Stop replacing unfinished
  movement, charged to
  the **previous** issuer. The first replacement of a restored order has no
  measured previous issuer and is excluded.
- `superseded_10s`: the subset replaced before ten simulation seconds elapsed
  since the previous accepted request, charged to that previous issuer. The
  requested redundancy sum is `near_reissues + superseded_10s`.
- `route_requests`: route-admission calls, including cached outcomes and
  slot refusals. A formation requests one shared route.
- `route_searches`: fresh planners created at route issue, excluding cache
  hits. Route continuation, frontier exploration and read-only probes are
  separate work and are not counted as new issue-time searches.

A synchronous issuer context surrounds external command application and each
manager pass. Other automatic trips (gathering, combat pursuit, rally, etc.)
fall into rally/automation. Diagnostics are absent from canonical hashes and
saves; per-unit history is discarded on death. They never drive decisions.
`scale-match` retains an initial sample and each wall-minute sample so rates
use counter **deltas**, excluding restore and AI connection time.
Samples also retain map-unit counts (including resident corpses) and movers
whose public state is moving or planning. These are distinct from scheduler
carriers: one formation carrier can represent many moving units.

## Reproduction

```sh
SCALE_MATCH_ARGS="--save .cache/order-volume/maps/saves/1794176817-first.save \
  --minutes 5 --port 8185 --package-directory .cache/order-volume/maps \
  --label orders-before-current --no-summary --out .cache/order-volume/before-current.json" make scale-match
```

The copied map store already contains the same verified saved pair. Select
that stored save directly: the importer deliberately refuses occupied names.
For the second run keep the save path, change the label to `orders-after-current`
and the output to `after-current.json`.
Both runs start from the same input. Wall-clock AI decisions and shared-host
load vary; this is neither dedicated-hardware qualification nor a controlled
causal timing benchmark.

## Saved-match observations

The baseline is clean revision `27669157963834272a1cdc5a5ec5b096c62929cb`.
The first after run is clean revision
`82ca47a405f5be86aa0dd86a96c206dc3a2cfdd5`. **That implementation failed
the full simulation suite and is rejected.** Its blanket one-tile suppression
also suppressed distinct gathering and combat contact points. Its numbers
below describe that experiment, not the corrected implementation.

Rates use the initial/final counter delta over the measured wall interval.
Each cell in the issuer table is baseline / rejected after, per wall minute.
Neither player nor auto-scout issued measured moves in this saved match.

| Issuer | Decisions | Unit moves | Near repeats | Reassigned <10s | Superseded | Route admissions | Fresh searches |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Player | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| Python AI | 2106.3 / 1341.2 | 2837.9 / 1949.7 | 438.8 / 75.1 | 818.2 / 250.6 | 596.5 / 58.5 | 1749.5 / 1026.3 | 1005.4 / 654.2 |
| Auto-scout | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| Steward | 270.8 / 169.1 | 230.5 / 150.2 | 0 / 0 | 184.8 / 121.4 | 30.3 / 13.2 | 204.5 / 129.0 | 204.5 / 129.0 |
| Vassal | 176.0 / 120.4 | 23.9 / 17.8 | 0.4 / 0.2 | 11.0 / 6.6 | 6.4 / 3.6 | 23.9 / 17.6 | 23.9 / 17.6 |
| General | 444.2 / 49.1 | 461.0 / 183.5 | 63.1 / 46.7 | 263.6 / 112.2 | 288.4 / 107.2 | 59.1 / 20.8 | 59.1 / 20.8 |
| Rally/automation | 15122.9 / 10972.3 | 15122.9 / 10972.3 | 263.6 / 600.1 | 5067.5 / 3268.1 | 663.1 / 333.9 | 11009.7 / 7958.5 | 6038.8 / 4047.2 |

| Measured quantity | Baseline | Rejected after |
| --- | ---: | ---: |
| Wall seconds | 300.668 | 300.447 |
| Simulation ticks | 4590 | 3089 |
| Ticks/s | 15.266 | 10.281 |
| Mean tick ms | 59.833 | 96.487 |
| Mean world advance ms | 43.194 | 70.269 |
| Unit movement intents/min | 18676.2 | 13273.4 |
| Fresh issue-time searches/min | 7331.7 | 4868.7 |
| Waiting routes, initial | 933 | 968 |
| Waiting routes, wall minutes 1–5 | 341, 358, 241, 287, 331 | 320, 308, 220, 219, 244 |

The slower simulation invalidates a claim that fewer wall-minute requests
demonstrate improvement. Per 1,000 ticks, movement requests increased
20389.8 → 21517.0, near repeats increased 836.2 → 1170.6, supersessions
fell 1730.1 → 837.2, and fresh searches changed 8004.4 → 7892.5.
There is **no demonstrated throughput gain** from these runs.

The shared host had different load (observed one-minute load samples
20.18/11.90 during/end baseline versus 35.13/22.05 after). Two unsuccessful
browser recording attempts also connected during the baseline, whereas none
connected during the after measurement. These are confounders, not measured
causes of the timing difference. AI decisions use wall time too.

The largest raw trip source was automatic gathering/contact approach, including
unreachable fishing approaches. Such attempts are not all redundant manager
decisions. Changing pathfinding or unreachable-resource policy is outside this
change. `before-routes-mid.json` and `after-routes-mid.json` retain raw source
attribution; `before.json` and `after.json` retain the complete reports under
`.cache/order-volume/`.

## Movement and worker rules

Area movement commands retain their route, search frontier, deadline and
queued waypoints when the active goal is within one tile. Identical contact
points are also retained; distinct precise interaction points remain distinct
even within a tile, so workers can reach the resource and soldiers can enter
attack range. Attack-move formation contacts also require exact equality.
An arrival point newly reserved by another task is selected again.
Both precisions use the same routing engine. Individual
overrides still leave formations; queued commands and attack-mode or formation
membership changes remain meaningful.

General and Vassal sources retain unchanged army goals. Vassal return orders
still cancel a new pursuit. Auto-scout already selects its next goal only once
the current trip ends; a regression test covers arrival/failure behavior.
Wounded withdrawal compares the selected free arrival contact before issuing
an order. A retained trip rejoins its existing corridor through the ordinary
bounded local-refinement budget; its global search frontier and admission
deadline survive. Simply keeping an old local segment changed retreat timing
and failed the engagement gate. Repeating a live gather job preserves its
phase, work credits, partial loads and return trip; failed trips remain retryable.
Steward and Python AI productive rebalancing use a 20-game-second dwell and
leave workers in transit alone. Idle recovery and failed-job recovery retain
their existing prompt paths. Steward assignment tenure is canonical replay
state; diagnostic issuer history is not.

## Video evidence

The paired VP9 WebM recordings show the Steward kingdom fixture with twenty
working villagers and original art. They are **fixture footage**, separate
from owner-save profiling; owner-save recorder attempts did not produce usable
footage. The farm camera and visible work were checked using extracted frames.

- Baseline: `.cache/showcase/orders-before.webm`, SHA-256
  `59a2d0cad4a45987b1176e3f2af9142ceeef7ba83aa44aba9f378964a1cfc7ac`.
- First after (rejected revision `82ca47a405f5`):
  `.cache/showcase/orders-after.webm`, SHA-256
  `74a58ab3ccabb1881c211bbcc1f1f65ded987689b660229c7aa7fa246015cb0c`.

Storyboards and probe metadata are retained alongside ignored local evidence.
Attach recordings using `SHIP_VIDEO` if a later session publishes this branch;
this task does not authorize publication.

## Qualification

The rebased follow-up passes all 21 selected simulation tests, including the
five-variant General engagement gate, queued-goal retention, exact contacts,
arrival reservations, gather-job retention/recovery, diagnostics and one-/four-
worker replay. General engagement shares are 87.774%, 88.112%, 85.400%, 92.231%
and 89.035% (mean 88.510%); every unchanged wait limit also passes. The
instrumentation-only baseline passes with mean 88.786%. These deterministic
engagement observations are functional qualification, not timing measurements.
No engagement floor or canonical pin was changed.

The HTTP endpoint test passes the strict ten-second boundary, and all 23
scale-match harness tests pass. Commands use `NEXTEST_TEST_THREADS=2
RUST_TEST_THREADS=2 CARGO_BUILD_JOBS=2`, one build at a time. The required
`make hooks-install hooks-check fmt-check lint structure-check architecture-check`
passes; logs are `followup-gates-final.log`, `followup-simulation-final.log`,
`endpoint-green.log` and `report-green.log` under `.cache/order-volume/`.

The older rejected full run had 1236/1243 passing; the subsequent correction
still failed General engagement (1243/1244). Their retained logs and rejected
footage above do not qualify this follow-up. Full simulation/server suites,
current SDK integration, fresh paired owner-save profiles with the ten-second
counter, the one-percent instruction comparison, preflight, refreshed footage
and CI remain unverified at this increment. The old saved-match reports lack
the new counter and mover endpoints and cannot establish the requested 50%
reduction. Publication is outside this task's authorization.
