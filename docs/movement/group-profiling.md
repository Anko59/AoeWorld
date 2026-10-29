# PR 6 movement profiling

The comparison starts at PR 6, `1b5a3aa6a67a`. Its parent PR 5 is
`fe8586ee3ed2`. No arrival/combat assertion or canonical replay pin changes in
the optimization. The measurements below use the working source committed
with this document. They are synthetic shared-host measurements, not finished
RTS performance or dedicated hardware qualification.

## Reproduction

Run these serially; `AOE_HEAVY_SLOTS=1` gives a 12-CPU container on this host.
The JSON records the admitted CPU quota, Rayon workers and available CPUs.
`CARGO_PROFILE_DEV_DEBUG=0` bounds the command bootstrap's memory; it does not
change the release workload or bench profile.

```sh
CARGO_PROFILE_DEV_DEBUG=0 AOE_HEAVY_SLOTS=1 make perf-groups-profile
make perf-groups-annotate
make perf-groups-annotate GROUP_CALLGRIND_INCLUSIVE=no \
  GROUP_CALLGRIND_REPORT=reports/perf/group-callgrind-self.txt
CARGO_PROFILE_DEV_DEBUG=0 AOE_HEAVY_SLOTS=1 make perf-groups \
  PERF_GROUPS_ARGS='--coarse-groups --out reports/perf/group-coarse.json'
```

`perf-groups-profile` holds one slot for serial native timing and Callgrind.
`perf-groups` uses 200 Line armies of 30–79 militia, 10,900 units, 100 players,
seed 1, a 1,000-tile uniform map, fog on and unchanged orders. After 100 warm-up
ticks it measures 300 plain ticks, then 300 profiled ticks. Query spans are
inclusive, overlap and add clock/atomic overhead. `--coarse-groups` selects
prepare/propose/apply spans instead; it changes observation, not simulation.
Unselected spans are absent from the report, not reported as measured zeros.

The Callgrind bench imports the **same workload builder** as the CLI. It warms
200 full ticks, asserts all 10,900 units are moving, and instruments only tick
201's movement phase. It stops before spacing/combat/remaining phases and
teardown. It uses one Rayon worker so the instruction count covers all work
without instrumenting unrelated worker startup. Native timing uses 12 workers.
The pinned analysis image uses Callgrind 3.19.0; bench debug information enables
inline attribution. Compare identical instrumentation and compiler settings.
The before Callgrind run used detached `1b5a3aa6` with only this bench,
shared workload builder, bench manifest entry and thin Make targets overlaid;
production movement stayed unchanged. The fresh native baseline was recorded
with diagnostics added before production optimizations. The raw before/final
profiles are retained locally as `group-pr6-debug.callgrind` and
`group-final.callgrind` under `reports/perf/`.

The raw profile and JSON summary are under
`target/gungraun/aoe-simulation/group_instructions/simulation/movement/`.

## Findings

PR 6 added repeated hard-body validation to the parallel proposal and ordered
apply paths. `avoidance.rs::clearance` costs 45,013,859 inclusive instructions;
that name denotes a swept collision wrapper, **not** the PR 5 clearance field.
The actual field lookup costs 204,852 instructions (0.21% of movement).
Slot assignment and route searches do not execute in this measured tick.
Allocator entry points account for less than 1%; allocation is not the source
of the extra six milliseconds. This open workload does not measure obstacle
replanning or worst-case allocation under repeated commands.

The optimization:

- Bounds unit sweeps by the largest observed land/water body. The bound
  grows at insertion and stays conservative after removal, eliminating the
  per-tick population scan. An inflated sweep
  certifies separation despite any earlier member's bounded step. Failed
  certificates still use the ordinary ordered recheck. Overfull cells retain
  the existing whole-tile policy, including when an earlier write creates one.
- Reuses the group's static clearance sample to prove an empty tile rectangle.
  The proof covers body radius plus a complete step; elevation edges still get
  checked. Outside the rectangle, ordinary static sweeps remain active.
- Prepares disjoint groups in parallel while retaining canonical group/member
  apply order. Computes pure member distance bookkeeping in proposals and
  recomputes it whenever ordered collision handling changes the position.
- Uses canonical member ordering for dense indexing/binary search, avoiding
  per-member hash lookups. Reuses heading normalization, with exact cardinal
  arithmetic and unchanged general integer division/rounding.
- Projects movement speed without computing unrelated combat stats, uses the
  existing integer cell hasher for wildlife's point-only lookup, and writes
  only group motion fields during apply. Spatial/vision updates remain ordered.
  Tile-only indices skip relocation within a tile; wildlife's exact-position
  mirror still updates on every step.

A certificate is never accepted from a snapshot containing overfull cells:
sampling 32 bodies cannot prove anything about omitted bodies, even if an
earlier ordered write removes overflow. A regression test first failed at that
certificate assertion, then passed with the snapshot guard.

An exact reciprocal-division cache was tested and rejected: it increased the
kernel from 44,922,429 to 47,514,514 instructions and measured 5.310 ms p50.
No approximate arithmetic or output pin regeneration was accepted.

## Results

The fresh PR 6 native baseline is `reports/perf/group-opt-before.json`; final
is `group-final-radius.json`, with a separate repeat/stage report in
`group-final-coarse.json`. Both final reports confirm 12 CPUs / 12 workers.
Every compared plain window ends at tick 400 with 10,900 moving units and 200
active entries. The PR 6 reports end at tick 700 with 8,364 moving / 390 active
entries and no movement failure. The later profiled window therefore has
different activity from the plain window.

| Plain window, ms p50 / p95 | PR 5, historical matched slot | PR 6, fresh baseline | Final | Final repeat |
| --- | --- | --- | --- | --- |
| Movement | 4.895 / 5.254 | 10.934 / 12.824 | **4.835 / 5.756** | **4.751 / 5.717** |
| Whole tick | 5.100 / 5.482 | 11.672 / 13.524 | 5.507 / 6.407 | 5.429 / 6.417 |

Both final movement p50 measurements meet 4.9 ms. The p95 remains above the
historical PR 5 measurement. This shared host does not establish a hardware
qualification. The cut before caching radii measured 4.918 / 5.993 and
4.922 / 6.116 ms: these were misses, not rounded down to a pass.
An earlier cut measured 4.810 / 5.946, then 4.974 / 6.309. Other earlier runs
measured 6.710 / 11.057 and 7.942 / 12.105 with spacing/collation roughly
50–80% slower too; another measured 10.010 ms p50. These measurements are
retained as shared-host variability, not silently discarded samples or a
reason to shrink the workload.

Callgrind counts **96,413,953 → 44,116,679 Ir**, a **54.24%** reduction. The
following are exclusive instructions attributed to source files (including
inline source attribution), not sums of overlapping inclusive call trees.
Library helper rows explain where the eliminated sweep work spent instructions.

| Source / cost | Before Ir | After Ir |
| --- | ---: | ---: |
| `movement_step/collision.rs` | 18,718,707 | 4,665,436 |
| Core integer helpers (`num/int_macros.rs`) | 14,029,997 | 3,584,364 |
| Core integer square root | 3,725,383 | 1,403,951 |
| `movement_step/static_collision.rs` | 2,361,225 | 19,823 |
| `formation/path.rs` | 2,435,314 | 3,131,327 |
| `formation/steering.rs` | 2,483,936 | 2,316,700 |
| `combat/military.rs` | 2,698,706 | 1,025,780 |
| SipHash | 1,924,787 | 0 |

Path-source instructions increased; total preparation and movement still fell.
Inlining/cardinal specialization move instructions between source rows, so
individual rows are not independent estimates of elapsed time. Inclusive
entries corroborate the hot paths: unit sweep 29,036,621 → 9,443,000; ordered
apply 35,308,538 → 8,768,517; static group check 9,979,218 → 1,007,361.
These entries overlap; do not add them to the exclusive table or each other.

Plain-window phase timings (ms p50 / p95) follow. The before report did not
sample the return tail: its zero was unmeasured, not a measured zero. The final
report records that tail using the existing `perf-scale` timing pattern.

| Phase | Before | Final |
| --- | --- | --- |
| `building_memory_per_player` | 0.002 / 0.002 | 0.002 / 0.002 |
| `combat` | 0.012 / 0.015 | 0.011 / 0.014 |
| `construction_and_gates` | 0.002 / 0.002 | 0.002 / 0.003 |
| `gathering_and_economy` | 0.002 / 0.002 | 0.002 / 0.002 |
| `idle_tail` | unmeasured | 0.002 / 0.003 |
| `movement_and_path_planning` | 10.934 / 12.824 | 4.835 / 5.756 |
| `research_and_training` | 0.002 / 0.002 | 0.002 / 0.002 |
| `result_collation` | 0.208 / 0.313 | 0.179 / 0.231 |
| `unit_spacing` | 0.475 / 0.632 | 0.447 / 0.518 |
| `vassal_economy` | 0.001 / 0.002 | 0.001 / 0.002 |
| `wildlife` | 0.004 / 0.005 | 0.004 / 0.004 |

Across the 300 profiled ticks, query spans change as follows. The windows have
the same commands/activity census, but per-query timing includes instrumentation
and parallel overlap and is not the plain movement time.

| Span | Before calls / inclusive ms | Final calls / inclusive ms |
| --- | --- | --- |
| Avoidance query | 6,732,422 / 14,061.818 | 4,165,472 / 8,366.352 |
| Sidestep | 170,629 / 1,696.772 | 170,629 / 1,731.874 |
| Local refinement | 60 / 10.243 | 60 / 10.250 |
| Arrival search | 1,773 / 2.318 | 1,773 / 2.274 |

The fine-profile window measures movement at 11.057 / 37.392 ms p50/p95,
versus 47.646 / 59.712 before. Its clock/atomic overhead is substantial.
The separate coarse repeat collects these stages instead of query timers:

| Group stage | Calls | Total ms | Mean ms/tick |
| --- | ---: | ---: | ---: |
| Prepare | 300 | 178.750 | 0.596 |
| Propose | 300 | 371.623 | 1.239 |
| Ordered apply and finalization | 300 | 805.256 | 2.684 |

Before-stage spans were not collected; they are not fabricated zeros.
The coarse spans cover the later window and are not a decomposition of the
plain-window p50. Ordered apply remains the largest stage.

The exact scenario assertions remain: open first/last 2222/2233 (11 ticks),
forest pass 2803/2879 (1.289×), attack-move kills all ten by tick 1512 with all
58 survivors at their destination, and healthy arrivals 1663–1960 / first hit
1731 / sealed give-up 1903. Both strict hit-before-tail and hit-before-give-up
checks remain. No existing assertion was loosened or deleted.

## Validation

Checks use Dockerized Make targets, serially, with `AOE_HEAVY_SLOTS=1` and
`CARGO_PROFILE_DEV_DEBUG=0`:

- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'`: 713 passed, none skipped; the unchanged 14,000-unit checkpoint pin passes.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server AOE_NATIVE_TEST_FILTER='test(/formation|command|group|combat/)'`: 17 passed, 314 outside the requested filter.
- `make lint`: native Clippy with warnings denied and WASM test-target compilation.
- `make hooks-install`, `make hooks-check`, and the indexed pre-commit checks.
- `make preflight`: static checks, the full 12-crate workspace suite with its existing tier exclusions, doc tests, three generated late-game replays and network smoke. Dev-server port, if needed, is 8165.

No deterministic pin is regenerated. Local checks and synthetic profiles do
not supply CI evidence or dedicated hardware qualification.
