# Radius movement: implementation and local evidence

## Revisions and acceptance status

Played base: `80ff4af26d2ad0052b91c419bc187e7e959a15c2`.
Dev comparison: `24ee53b9cc3a511bfa2ed4d06faa5ed1f3c22b5f`.
Research was committed before implementation:
`4acf98bc701d497ac0293c2cf2d3c09f212bc7cc`.
Implementation commits: `fb05b29c` (radius movement), `9cc85553`
(exact static sweeps and intermediate rejoining), `0b3a6da5` (distant anchors),
`066d2db1` (lateral escape, elapsed retries), `90a3d303` (friendly
squeeze-through, worksite-only redirects) and `ed856700` (its regression).

The seeded-scout regression and all requested scenarios pass. Late-game tick
time and instructions improve on the played base; `make perf-ci` still reports
Regression against dev's pinned instruction, allocation-count and wasm-size
baselines (see below). This report does not qualify a finished RTS or
dedicated hardware.

## Verified causes and replacement

The [researched model](aoe2-model.md) cites Pottinger's two 1999 articles,
Marselas's Ensemble retrospective, official DE release notes, and the inspected
classic DAT. Published principles have high confidence. Exact yielding,
finite spot layouts and work reach are AoeWorld choices; proprietary DE
implementation parity is not established.

Verified in the played source:

- Whole-tile arrival/work reservations rejected physically legal packing.
- One 640-subunit avoidance clearance and a separate 768-subunit correction
  ignored actor size. Villagers have a verified classic DAT radius of 0.2 tiles.
- Two-worker wood and eight-worker other-resource admission caps caused queues.
- A gather transition could begin work after its optional last step failed.
- Local detours rounded fractional positions to tile centres.
- Formation cohesion could stop the anchor while rear members could not catch up.
- Local searches had per-actor retries without a shared tick allowance; near
  attack searches also escaped the previous far-search-only allowance.

The replacement uses DAT-sized circles, exact swept collisions, exact route
positions/reservations, contact-checked work, radius-spaced melee perimeter
positions, existing ranged reach, remembered steering and yielding, and a
formation anchor that keeps moving at a reduced pace when stretched.

Additional failures were reproduced during integration. A diagonal segment
between clear endpoint circles could cut a static corner: the planner accepted
an edge that actual small steps could not execute. Exact static sweeps now
reject that chord. A seeded scout also repeatedly approached a clipped local
objective inside the Town Center while retaining a distant legal waypoint.
The bounded local search now ranks reachable positions against the actual
distant waypoint instead of substituting the next tile. That corrected the
worker-row fixture but did not close the exact seeded case: resources above
the row left the lattice repeatedly choosing the same pocket. Sustained
nonproductive refinement now tries a remembered lateral escape, then a fresh
static route. It must gain at least half a tile laterally, spends the existing
shared search allowance, and preserves the final destination and command queue.

Normal collision queries examine at most sixteen one-tile cells and 32 bodies
per cell. Overfull spawn cells reject incoming traffic and permit bounded
outward repair; that exception is not a proof of collision-free packing of
thousands of identical spawns. Domain-separated packed keys, recycled empty
cell vectors and a dense entity-to-unit lookup reduce query/allocation cost.

The world shares four local refinements of at most 306 nodes and four expensive
attack-position searches of at most 8,192 nodes each per tick. Formations retain
two local-search allowances while present. The existing fair static planner
keeps its 16,384-work allowance; fast attempts keep 65,536 work globally and
16,384 per player. Generated detours cannot grow a route beyond 512 points.
No performance threshold or workload floor was reduced.

## Scenario and integration evidence

Eleven acceptance regressions were observed failing on the played base before
production changes. The focused movement run passed 68/68. Subsequent focused
runs include the same seven requested scenarios, exact corner sweeps, the
initial scout, long-segment joins and occupied intermediate points.

| Scenario | Maximum stationary wait, ticks | Required completion |
| --- | ---: | --- |
| Six lumberjacks, three trees | 200 | Every worker collects at contact |
| Ten villagers, one gold mine | 200 | Every worker collects at contact |
| Twelve militia, House | 160 | Every militia strikes |
| Eight archers | 160 | Every archer fires within range |
| Two crossing groups of twenty | 160 | Every actor arrives |
| Four Fishing Ships and two Galleys, three-tile channel | 160 | Every ship arrives |
| Twenty-unit formation, two-tile gap | 240 | Every member arrives |

These tests check each actor's progress and physical clearance during the run,
not just the leader or a final unit count. Work tests check actual carried
resources and contact distance; melee tests check every attacker's strike.

The only full native run was `AOE_TEST_ALL=1 make preflight`:
3,498 tests, 3,485 passed, 13 failed, 18 skipped (395.085 seconds).
It was not repeated. The failures identified stale current replay pins, two
attack fixtures/assertions, the old waypoint-cadence fixture, an impassable
target's contact ring, village wood/fishing approaches, predator discovery,
the SDK two-bot scout, and an optional minimap dump timeout.

After corrections, the 88-case focused run passed 81; its seven failures were
six expected pin comparisons and the SDK scout. Refreshing the current pins
then yielded 23/24 in a focused rerun, with only the SDK scout failing.
Independent replay and physical-cache-mode comparisons passed before their
final expected-checkpoint assertions. The late-game one/four-thread comparison
also passed before its deliberately changed checkpoint was refreshed.

The SDK failure was reproduced without networking on the same flat map and
village seed: scout `(412741, 369311)`, HP 45, active movement order,
1,182 blocked ticks. It is a movement failure, not a dead scout or lost command.
The anchor-only correction passed one of two focused reproductions; the exact
flat village still failed. The lateral escape correction is tested separately.

Replay hashes and wire-change digests were regenerated because radii,
fractional route points, persistent reservations, search accounting and local
scheduling intentionally change canonical output. Workloads, activity floors,
whole independent runs, thread comparisons and physical-cache comparisons
remain present. The complete actor/order remains unchanged on a refused
attack; spending the explicitly shared search allowance changes its hashed
budget accounting, so that fixture checks actor/order preservation directly.

The optional stored-minimap raster dump exceeded its unchanged 30-second
budget (138.846 seconds) with the copied real map inputs on this shared host.
That is outside movement scope and remains a full-suite failure. The inputs
were not removed to suppress it.

## Commands and static checks

All compilation/checks used Dockerized Make targets and the worktree's normal
`target/`. Focused iterations used `make test-native-focused` with package and
nextest filters. The full native suite ran only once through preflight.

Completed passing checks: `make fmt`, `make structure-check`,
`make test-placement`, `make lint`, `make architecture-check`, `make docs-check`,
`make hooks-install`, `make hooks-check`, and signed-commit pre-commit hooks.
Lint checks native targets with warnings denied and compiles wasm test targets.
Preflight's wasm build passed. No web/client production code changed, so the conditional
browser-check, test-wasm execution and focused browser e2e were not run.

Early preflight stopped on two prohibited panic calls, replaced with checked
handling. Architecture checks then exposed two test bindings still naming the
old inline waypoint tests; the catalog and its AST test now name the separate
test file. Claims, cases and assertions remain intact. Those checks passed.

One corrective commit attempt failed because incremental compiler cache files
disappeared. Retrying with `CARGO_INCREMENTAL=0` passed the unchanged hooks.
No extra target cache was created, and no hooks were bypassed.

The requested five-hour wall-time box was exceeded. Individual final focused
jobs waited 1,036 and 1,404 seconds for shared heavy slots. Slot capacity was
not changed and this session queued only one heavy target at a time.

## Performance evidence

All measurements share this development machine (6 slot CPUs) with other
agents; base and current runs below were taken back to back in the same
session by checking out the base sources in the same worktree. Target-hotspot
has four tick samples, so nearest-rank p95 is its maximum.

Late-game diagnosis (seed workload, ticks 320-384): obstructed work approaches
re-issued their approach 8,531 times in 64 ticks, each a fresh static route
search with a destination pocket proof (base: 906 route orders). Restricting
redirects to the worksite restored 767 orders. That exposed 237 units standing
still with an order for 64+ ticks (base: 10) in compressed village crowds;
friendly squeeze-through reduced it to 5.

| Workload (ms) | Base 80ff4af2 p50 | Base p95 | Current p50 | Current p95 |
| --- | ---: | ---: | ---: | ---: |
| `make perf-late-game` simulation tick | 79.117 | 102.352 | 37.539 | 55.381 |
| `make perf-late-game` server tick | 78.285 | 99.922 | 45.146 | 64.178 |
| `make perf-smoke` late-game (base: earlier 4-CPU run) | 70.547 | 103.722 | 48.026 | 61.605 |
| `make perf-scale` tick with orders | 12.383 | 31.986 | 15.477 | 116.519 |
| `make perf-scale --fight-size 0` | 10.382 | 24.012 | 11.225 | 90.109 |

One late-game tick under Callgrind: 430,571,870 instructions on the base,
556,395,207 at `066d2db1`, 247,387,783 now (dev baseline 143,767,781).

The perf-scale regression is a longer startup burst, not a costlier tick.
Its setup admits 6,079 gather jobs instead of 2,196 because the two-wood and
eight-ore caps are gone, and its fights now find attack positions (964
attackers instead of 13). Per-tick timelines show both builds at about
100-160 ms while thousands of units wait for the fixed route budget; the base
burst lasts about 22 ticks with 2,700 movers, the current one about 45 ticks
with 6,600. After the burst both run at about 4-5 ms per tick and p99/max are
similar (base 144/214 ms, current 155/224 ms). This workload then collapses to
under 100 active movers in both builds, so it cannot establish RTS
performance.

`make perf-ci` at `ed856700`: target-hotspot p50 1.222 / p95 3.125 ms (dev
24ee53b9: 1.840 / 1.951; owner-reported base about 11 ms); target-distributed
0.591 / 2.930 (dev 1.894 / 4.991). Instruction, allocation-count (6,306 vs
5,671) and gzip wasm (302,979 vs 287,702 bytes) comparisons are Regression
against dev's unchanged 5% allowances; allocation bytes pass. No baseline or
allowance was changed.

Raw reports are ignored local files under `reports/perf/path2/` and the
session scratchpad; they retain their actual revision and dirty flags.

## Limits

Original game assets and derived images remain outside Git and Docker images.
No browser playtest or dedicated-hardware qualification was performed. A finite
local policy cannot guarantee progress in a physically sealed or permanently
full region. The public sources do not disclose the entire AoE2/DE pathfinder.
The classic DAT dimensions were verified; exact DE obstruction policy and
AoeWorld's yielding/work-reach constants remain explicitly separate claims.
