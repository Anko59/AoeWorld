# Radius movement: implementation and local evidence

## Revisions and acceptance status

Played base: `80ff4af26d2ad0052b91c419bc187e7e959a15c2`.
Dev comparison: `24ee53b9cc3a511bfa2ed4d06faa5ed1f3c22b5f`.
Research was committed before implementation:
`4acf98bc701d497ac0293c2cf2d3c09f212bc7cc`.
Implementation commits: `fb05b29c` (radius movement), `9cc85553`
(exact static sweeps and intermediate rejoining), `0b3a6da5` (distant anchors).

Acceptance is incomplete until the final seeded-scout regression and final
performance measurements below are closed. The last measured intermediate
implementation failed performance gates. This report does not qualify a
finished RTS or dedicated hardware.

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

All measurements share this development machine with other agents. The dev
Make target used its debug profile; played/current targets use release. The
target-hotspot workload has four tick samples, so nearest-rank p95 is its
maximum, not a stable distribution estimate.

| Workload | Reference p50 ms | Reference p95 ms | Intermediate p50 ms | Intermediate p95 ms |
| --- | ---: | ---: | ---: | ---: |
| Dev target-hotspot | 1.840 | 1.951 | 1.543 | 3.291 |
| Played scale, 600 ticks including orders | 13.797 | 32.755 | Unmeasured | Unmeasured |
| Played late-game simulation, pooled three runs | 64.726 | 82.827 | 80.769 | 94.744 |
| Played late-game server, pooled three runs | 70.547 | 103.722 | 63.506 | 70.410 |

The intermediate measured production patch SHA-256 is
`60905c5475d915f05ca04c2f1124d573fa15c0f205e26584fb4b011779307ac9`.
It precedes the collision-index optimization, exact static sweeps and final
distant rejoin correction. These numbers must not be attributed to later code.

That `make perf-ci` returned Regression: late-game instructions were
627,454,922 against 143,767,781 and allocations 10,433 against 5,671, each with
an unchanged 5% allowance. Allocation bytes passed (3,383,987 vs 4,127,477).
Optimized gzip wasm was 302,979 bytes against 287,702 and exceeded 5%.
The hotspot p95 also missed the requested dev comparison. Callgrind attributed
about 48% of that measured tick to static leg searches, including destination
pocket proofs. Bounded work alone does not demonstrate a faster tick.

The scale baseline ended with only 48 active movers. A collapsed movement
workload cannot establish RTS performance; activity counts must accompany
timing. Late-game workload activity floors are retained.

Raw generated evidence is under ignored `reports/perf/path2/`, including
`before-scale.json`, `before-smoke.json`, `dev-ci.json`,
`intermediate-ci.json`, and the initial-after Callgrind profile. Reports retain
their actual revision/dirty flags. The played smoke baseline's clone reported
dirty because its Git alternate was inaccessible inside Docker; the scale
baseline has the same metadata limitation. Their host source was clean. This
metadata is preserved, not rewritten.

## Limits

Original game assets and derived images remain outside Git and Docker images.
No browser playtest or dedicated-hardware qualification was performed. A finite
local policy cannot guarantee progress in a physically sealed or permanently
full region. The public sources do not disclose the entire AoE2/DE pathfinder.
The classic DAT dimensions were verified; exact DE obstruction policy and
AoeWorld's yielding/work-reach constants remain explicitly separate claims.
