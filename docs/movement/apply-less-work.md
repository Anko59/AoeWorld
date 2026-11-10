# Ordered movement: doing less work

The ordered apply remains serial. No replay pins are changed. The workload is
[match-bench](../scale/match-bench.md): 100 seats, seed 1, owner density, load
factors 1 and 5, independently replayed with 1 and 4 Rayon workers.

## Reproduction

```sh
NEXTEST_TEST_THREADS=2 RUST_TEST_THREADS=2 CARGO_PROFILE_DEV_DEBUG=0 make movement-apply-census
```

This is an explicitly selected, otherwise ignored in-process profiling test,
without the long-job slot. It is outside the ordinary unit-test runtime budget. It builds each
world and plays 150 closed-loop ticks. Ticks 101–125 measure the whole movement
phase without a planning probe. Ticks 126–150 separately collect inclusive
body-sweep, sidestep, settling and ordered-loop spans. Timer allocation and
observer synchronization are included in those fine spans: their per-call costs
are diagnostic, not an estimate of uninstrumented nanoseconds. All measurements
use the repository's optimized simulation test profile, with assertions and
overflow checks enabled; they are not release-build numbers.

The ignored `.cache/apply-less/latest.json` records wall means/medians, work
counts, span nanoseconds, setup time, host load, and all fifteen ten-tick hashes.
Copy it before rerunning to retain a before/after pair. The test asserts worker
hash equality. Cross-revision equality must also compare every checkpoint.

`applied` counts each successfully applied proposal or sequential fallback once;
for a multi-waypoint fallback the census uses its first segment. `applied_clear`
requires no other unit centre in any cell intersecting that segment's swept
bounding box expanded by both collision radii, plus the ordinary static body,
terrain-edge and building checks. Opposite movement layers cannot collide.
`applied_stationary` counts zero-length first segments. `individual_turns`
also includes route-carrier turns and units that settle before physical apply.
Spans cover queries in both proposal and apply; their call counts can exceed
movers. Sweeps inside sidesteps contribute to both columns: the costs are not
additive. Settling and sidestep also overlap the ordered loops.

The cell census ignores only a singleton actor cell. Another body anywhere in a
queried cell vetoes the proof, including dead bodies and bodies outside the exact
swept shape. Insertions, removals and within-cell relocations use the live
collision index. Static clearance is a separate requirement; empty unit cells
never establish terrain or building passability.

These are short synthetic owner-shaped matches on a shared machine, not an
owner-save replay, finished RTS performance or hardware qualification. This
performance-only change must preserve visible movement; no game behavior is
intentionally changed.

## Baseline census (2026-11-10)

Before any fast path, simulation behavior from `0df71a07c2e2`, plus the observer
instrumentation in this change. The 1x runs have 9,619 units; the 5x run has 46,691 units.
At 1x the 1/4-worker checkpoint sequences agree (15 checkpoints each).

| Scale | Workers | Applied/tick | Clear/tick | Stationary/tick | Movement mean ms | Movement p50 ms |
|---|---:|---:|---:|---:|---:|---:|
| 1x | 1 | 710.40 | 85.72 | 427.04 | 20.311 | 4.365 |
| 1x | 4 | 710.40 | 85.72 | 427.04 | 18.311 | 4.033 |
| 5x | 1 | 10,499.52 | 362.88 | 9,187.56 | 120.776 | 105.800 |

Only 12.1% of applied movers pass this deliberately conservative clearance
proof; 60.1% have a zero-length first segment. Stationary does not imply an
inert apply: carry, arrival, planning and formation bookkeeping can still change.

Inclusive microseconds normalized by applied movers (not just query calls):

| Scale | Workers | Body sweep us/mover | Sidestep us/mover | Settling us/mover |
|---|---:|---:|---:|---:|
| 1x | 1 | 4.650 | 4.502 | 0.598 |
| 1x | 4 | 5.993 | 4.922 | 0.615 |
| 5x | 1 | 2.892 | 1.641 | 1.557 |

There are 1,273.64 sweep, 144.64 sidestep and 205.76 settling calls per tick.
At one worker their inclusive means per call are 2.593, 22.109 and 2.065 us.
The one-minute host loads at setup were 34.08 and 31.15. Means include route
outliers and differ sharply from the medians; this is not a worker speedup claim.

At 5x, 3.46% pass the cell/static proof and 87.5% have a stationary first
segment. The 5x four-worker replay and the optimized measurements are pending
in this instrumentation increment.
