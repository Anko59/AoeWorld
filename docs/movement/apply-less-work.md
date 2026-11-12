# Ordered movement: doing less work

The ordered apply remains serial. The apply changes alone change no replay pins; the order-retention change shipped with them repins the late-game replay (see `docs/scale/order-volume.md`). The workload is
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
At both scales the 1/4-worker checkpoint sequences agree (15 checkpoints each).

| Scale | Workers | Applied/tick | Clear/tick | Stationary/tick | Movement mean ms | Movement p50 ms |
|---|---:|---:|---:|---:|---:|---:|
| 1x | 1 | 710.40 | 85.72 | 427.04 | 20.311 | 4.365 |
| 1x | 4 | 710.40 | 85.72 | 427.04 | 18.311 | 4.033 |
| 5x | 1 | 10,499.52 | 362.88 | 9,187.56 | 120.776 | 105.800 |
| 5x | 4 | 10,499.52 | 362.88 | 9,187.56 | 102.193 | 87.020 |

Only 12.1% of applied movers pass this deliberately conservative clearance
proof; 60.1% have a zero-length first segment. Stationary does not imply an
inert apply: carry, arrival, planning and formation bookkeeping can still change.

Inclusive microseconds normalized by applied movers (not just query calls):

| Scale | Workers | Body sweep us/mover | Sidestep us/mover | Settling us/mover |
|---|---:|---:|---:|---:|
| 1x | 1 | 4.650 | 4.502 | 0.598 |
| 1x | 4 | 5.993 | 4.922 | 0.615 |
| 5x | 1 | 2.892 | 1.641 | 1.557 |
| 5x | 4 | 4.616 | 1.684 | 1.662 |

There are 1,273.64 sweep, 144.64 sidestep and 205.76 settling calls per tick.
At one worker their inclusive means per call are 2.593, 22.109 and 2.065 us.
The one-minute host loads at setup were 34.08 and 31.15. Means include route
outliers and differ sharply from the medians; this is not a worker speedup claim.

At 5x, 3.46% pass the cell/static proof and 87.5% have a stationary first
segment. 

## Exact fast paths

- The original grid traversal itself proves absence of relevant bodies:
  empty cells, the actor, dead bodies and centres outside the swept box never
  construct contact policy or run body geometry. A crowded query scans its
  cells once, rather than first rescanning them for an emptiness proof. The
  original whole-cell compressed-spawn policy remains. Every policy-free query
  in unit tests executes the eager full path and asserts the same answer.
  Static terrain/building checks remain at the existing callers.
- An unchanged individual physical state can skip physical apply, vision,
  spatial relocation and progress observation only with no obstruction, route
  pop or stale vision, and an unchanged bucket. The live arrival reservation
  check remains. Required order/carry updates and mover-list ordering remain;
  when the order is also unchanged, apply writes nothing. The test oracle runs
  full apply and compares the unit, order, route, planning age, bucket and output
  lists. No movement is skipped merely because this tick's displacement is zero.
- An unobstructed mover skips settlement; tests execute the full settlement
  path and assert it leaves the same fields unchanged and returns false.
- A group proposal batch caches the static "tile has no exit" answer by tile
  and unit kind. It computes each key once, only outside existing static-area
  certificates. Precise bridge proximity stays per position. The cache is
  cleared before the next batch; tests compare every hit with the uncached
  answer and rebuild after new walls. Live body emptiness is never cached
  across ordered writes.

The optimized census also records `applied_no_body`: no other living same-layer
unit intersects the exact swept capsule, and the ordinary static checks pass.
It scans every indexed body in the candidate cells, including those beyond the
32-body cyclic decision sample. Zero-length capsules include initial overlaps;
tangency is clear. This diagnostic does not make movement decisions. Its extra
work is outside the uninstrumented wall window. Coarse instrumented apply-loop
times include it and must not be compared directly with the old census.

Oracle verification is outside engine collision-work accounting, in nested,
unwind-safe worker-local test scopes. The fused traversal counts all original
cell/body inspections without a duplicate emptiness scan. Four current-output
collision-count pins decrease deliberately (river 3,108,113 → 3,107,314; town
2,510,834 → 2,510,772; crossing 3,051,861 → 3,051,342; long march 102,402,561 →
102,322,464). Their arrival ticks, route work, behavior/throughput bounds and all
canonical-hash pins remain unchanged. These are query counts, not CPU cycles.

## Before/after results (2026-11-10)

Optimized simulation: `a7e072abbaaf36951a006b7d92c704cdd8959d32`.
The same plain ticks 101–125, profile ticks 126–150 and all 60 canonical
checkpoints were used. All baseline comparisons passed; neither world-hash
pins nor behavior limits changed.

| Scale | Workers | Before mean ms | After mean ms | Before p50 ms | After p50 ms |
|---|---:|---:|---:|---:|---:|
| 1x | 1 | 20.311 | 19.193 | 4.365 | 4.848 |
| 1x | 4 | 18.311 | 12.061 | 4.033 | 3.924 |
| 5x | 1 | 120.776 | 103.698 | 105.800 | 84.640 |
| 5x | 4 | 102.193 | 104.121 | 87.020 | 86.060 |

Wall results are mixed: the 5x single-worker mean falls 14.1%, but the 5x
four-worker mean rises 1.9%. The 1x single-worker median also rises. One-minute
setup loads before/after were 34.08/28.31, 31.15/40.08, 29.07/23.84 and
18.28/24.44 respectively. These short shared-machine windows do not establish
a consistent speedup or isolate the cause of timing differences.

Exact capsule/static census and stationary apply, per tick (counts identical
at 1/4 workers):

| Scale | Applied | No body + static clear | Share | Stationary | Bookkeeping-only apply |
|---|---:|---:|---:|---:|---:|
| 1x | 710.40 | 646.96 | 91.07% | 427.04 | 180.28 |
| 5x | 10,499.52 | 9,971.12 | 94.97% | 9,187.56 | 7,905.36 |

The exact census finds far more clear paths than whole-cell absence does.
A stationary mover can still owe fractional credit: no wholly inert no-op
apply was observed in these windows. The shortcut commits the required order
without repeating physical state, vision, bucket or progress writes.

Function calls and cache reuse, per tick (both worker counts agree):

| Scale | Sweeps before → after | Settlement before → after | Contact policy skipped | Cached cell keys | Cache hits |
|---|---:|---:|---:|---:|---:|
| 1x | 1,273.64 → 1,093.36 | 205.76 → 16.52 | 267.44 | 487.72 | 506.32 |
| 5x | 13,560.44 → 5,655.08 | 7,998.40 → 82.44 | 1,415.88 | 2,212.92 | 2,505.44 |

At 5x, sweep calls fall 58.3% and settlement calls fall 99.0%; sidestep calls
remain 819.56/tick (144.64/tick at 1x). These call reductions demonstrate less
engine machinery, not the same percentage reduction in uninstrumented CPU time.

Inclusive diagnostic cost, before → after us per applied mover; sweeps nested
inside sidesteps are counted in both columns. Observer/timer overhead is not
subtracted, and these columns must not be summed:

| Scale | Workers | Body sweep | Sidestep | Settlement |
|---|---:|---:|---:|---:|
| 1x | 1 | 4.650 → 3.697 | 4.502 → 4.087 | 0.598 → 0.053 |
| 1x | 4 | 5.993 → 4.280 | 4.922 → 3.881 | 0.615 → 0.052 |
| 5x | 1 | 2.892 → 1.383 | 1.641 → 1.602 | 1.557 → 0.047 |
| 5x | 4 | 4.616 → 2.547 | 1.684 → 1.794 | 1.662 → 0.069 |

## Validation

All commands used `NEXTEST_TEST_THREADS=2 RUST_TEST_THREADS=2
CARGO_PROFILE_DEV_DEBUG=0` and Dockerized Make. No long-job slot was taken.

- `make hooks-install hooks-check fmt-check lint structure-check architecture-check`: pass.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'`:
  1,236 passed, zero skipped. This includes movement, formations, bridges,
  deadlock, five General-engagement cases, two formation-engagement cases,
  and the unchanged 14,000-unit canonical checkpoint at 1/4 workers.
- `make test-harness-focused HARNESS_TEST_FILTER=perf_scale::match_bench`:
  nine passed; the large census is ignored by ordinary unit runs.
- `make movement-apply-census`: the explicitly selected large census passed;
  all 60 saved baseline checkpoints agree, with worker equality at both scales.

The baseline run used the instrumented `0df71a07c2e2` simulation (the simulation
source committed in `98e3ccd68`). Its test invocation preceded the ignored-test
annotation and root-anchored artifact path; those changes affect selection/output
only. Raw reports remain outside Git in `.cache/apply-less/before.json` and
`after.json`. The optimized simulation source is committed separately from the
final measurement table.

No CI, owner-save replay, release-build timing or dedicated hardware run was
performed. No recording was made: the brief and launcher assign no `DEV_PORT`,
and the shared-machine rules permit only an assigned recording-server port.
A port request is pending. There is also no `local-assets` link in this worktree.
No assets or recordings are committed. Shipping is prohibited by this task.

## Owner-save timing observers (2026-11-12)

The Mekong save at `d0a1677ecb63d56a2a14d52e751d5ebc07e0a162`
revealed an unintended cost of attaching the server's timing observer.
`census_apply` ran its extra body census and static-clearance queries whenever
any planning probe was present, although the server discarded the resulting
counts. These queries are diagnostics, separate from movement's collision
decisions. Removing the observer itself would also remove required phase
measurements, so the geometric census now requires an explicit
`PlanningProbe::movement_census_enabled` request. The dedicated apply-census
observer enables it; timing spans and ordinary work counters remain available.

Before changing code, a two-minute `scale-match --save` run sampled the current
Mekong save with `--profile-after 30 --profile-seconds 60`: 26,977 process-CPU
samples at 499 Hz, with 11,748 to 11,796 units during the run.
Under `apply_step_full`, 1,176 of 4,361 samples (27.0%) included `census_apply`.
Across `advance_movement`, the census appeared in 1,535 of 11,665 samples
(13.2%). The remaining prominent apply functions were `adjust_group_step`,
`avoid_unit_step` and `collision_sweep_impl`, whose samples overlap.
This short initial profile identifies work; the ten-minute save benchmark is
the timing comparison. Raw profiles and save reports stay outside Git in the
benchmark checkout's `.cache/save-bench/`.

The regression first failed with 32 geometric census results on a timing-only
observer. It now checks that timing requests remain active without those
results, comparing every tick's output and canonical hash with an unobserved
world. The dedicated harness observer separately retains all 32 clear-step
census results, balanced apply spans and the same replay. Collision ordering,
movement decisions, save data and canonical-hash pins are unchanged.

The census opt-in alone did not qualify for a commit in its one ten-minute
save run (`apply3-census-optin.json`). Whole-run ticks/s fell from 10.034 to
9.758 (-2.8%). Median minute p50/p95 milliseconds were:

| Phase | Baseline | Census opt-in |
|---|---|---|
| group_apply | 16.550 / 23.858 | 16.419 / 22.027 |
| apply_loop | 18.036 / 27.595 | 16.178 / 24.167 |
| sidestep | 6.683 / 9.659 | 7.895 / 10.884 |
| index_collision | 5.328 / 6.277 | 5.567 / 7.188 |

The sum of those p50 summaries fell only 1.2% (46.597 to 46.059 ms).
These spans overlap, so that sum is not exclusive movement time. The first
attempt was not committed or remeasured unchanged.

The next change reuses collision-cell samples across one actor's sidestep
candidates. The index is immutably borrowed until that decision returns; no
sample survives an ordered relocation. Each cell retains its original cyclic
32-body window as two slices. Overflow policy and candidate-window rejection
still run for every query. A lazy 5-by-5 cache holds common nearby cells;
queries outside it read the same live index without a movement-distance cap.
Tests compare results, callback order, overflow handling and work counts with
direct queries, and every cached movement sweep checks the full geometry in
an unmeasured test oracle. No collision-work or canonical-hash pin changes.

The combined change's one ten-minute run (`apply3-neighborhood.json`) completed
6,671 ticks in 602.182 seconds: **11.078 ticks/s**, up **10.4%** from the
baseline's 10.034. Units grew from 11,748 to 11,991 (baseline: 11,918).
All three reports describe the identical input save; its stored bytes and card
also match the original input. Median minute p50/p95 milliseconds:

| Phase | Baseline | Census + neighborhood |
|---|---|---|
| group_apply | 16.550 / 23.858 | 12.572 / 17.615 |
| apply_loop | 18.036 / 27.595 | 14.107 / 22.697 |
| sidestep | 6.683 / 9.659 | 7.050 / 9.550 |
| index_collision | 5.328 / 6.277 | 4.999 / 6.031 |
| whole tick | 93.992 / 136.741 | 86.416 / 115.104 |

The four requested p50 summaries sum to 38.728 ms, down 16.9% from 46.597.
That remains above the 25 ms target, and sidestep p50 rose versus baseline.
The ticks/s result meets the brief's >5% first-commit stopping rule; this is
not a 20 Hz qualification. Planning cutover and owner rebasing are separate.

Both changed trees were measured once, after an exclusive machine-wide lock
and a load-below-six wait (initial loads 3.01 and 4.42). AI decisions and shared
machine conditions vary between runs; no isolated per-optimization causal or
dedicated-hardware claim follows. The phase summaries above use the same
aggregation across all minutes, rather than mixing the brief's final-minute
snapshot with a whole-run result. Timed spans remain enabled and inclusive.

Final-source Dockerized checks: `make fmt-check lint structure-check
architecture-check`, 231 focused simulation movement/formation/scale-hook
tests, and 10 focused `perf_scale::match_bench` harness tests passed. The large
census stayed ignored; no full suite, preflight, e2e or CI ran here.
Bench3 built an uncommitted source copy of the intended patch; SHA-256 checks
of all nine changed code files matched the primary checkout before and after
measurement. Save data, raw reports and recordings remain outside Git.
