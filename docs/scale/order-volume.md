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

## Reproduction

```sh
SCALE_MATCH_ARGS="--save .cache/order-volume/maps/saves/1794176817-first.save \
  --minutes 5 --port 8185 --package-directory .cache/order-volume/maps \
  --label orders-before --no-summary --out .cache/order-volume/before.json" make scale-match
```

The copied map store already contains the same verified saved pair. Select
that stored save directly: the importer deliberately refuses occupied names.
For the second run keep the save path, change the label to `orders-after` and
the output to `after.json`.
Both runs start from the same input. Wall-clock AI decisions and shared-host
load vary; this is neither dedicated-hardware qualification nor a controlled
causal timing benchmark.
