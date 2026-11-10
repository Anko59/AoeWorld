# Vassal bridge occupancy

The bridge span placement predicate now looks up each raster cell in the
existing building footprint index. That index includes unfinished and
nonblocking structures, and indexes angled bridges by raster cell rather than
by their bounding rectangle. No new persistent index or cross-tick placement
cache is introduced. Unit-dependent validation still runs for each candidate.

Road planning collects existing bridges through the player/kind index. Threat
selection visits enemy Town Centers through the same index, preserving the
distance/y/x choice. Neither operation scans unrelated buildings.

## Reproduction

Run through the Dockerized target:

```sh
CARGO_PROFILE_DEV_DEBUG=0 NEXTEST_TEST_THREADS=2 make test-native-focused \
  AOE_NATIVE_TEST_PACKAGE=aoe-simulation \
  AOE_NATIVE_TEST_FILTER='test(vassal::scale::)|test(bridges::occupancy::)'
```

The scale test constructs a 256-tile prepared river map, with a Castle and
Town Center on opposite banks, a garrisoned Vassal and 10,000 remote road
foundations. It measures three ordinary AI passes, forcing the normal
1,200-tick replan interval between them. No builders mutate the planning input.
It records validation calls, occupancy visits, validation time, total AI time,
the worst AI pass, the complete road/bridge plan and the canonical state hash
in ignored `.cache/cx-bridge/measurement.txt`.

The original scan at base `55f6357742b163a22b34ee9331ed612140440259`, with
test-only counters/timers added, produced:

| Measurement | Original scan | Indexed occupancy | Per-plan memoization |
| --- | ---: | ---: | ---: |
| Buildings | 10,002 | 10,002 | 10,002 |
| Bridge validations | 648 | 648 | 648 |
| Occupancy visits | 6,481,296 buildings | 3,831 tiles | 3,831 tiles |
| Validation total | 648.237 ms | 4.563 ms | 3.842 ms |
| Three Vassal passes | 1,029.297 ms | 321.613 ms | 186.866 ms |
| Worst Vassal pass | 353.178 ms | 128.722 ms | 76.490 ms |

The new occupancy work bound failed against that implementation, as intended.
The captured pre-change canonical hash is pinned in the regression:
`bf86364882cff97061b1c6dcc97acb200da621250ffb65b590d97dfe5acf870e`.
No existing hash fixture has been repinned.

Both runs produced that same hash and the same complete road/bridge plan.
The indexed run passed the occupancy bound and both new correctness tests.
The indexed lookup alone removes the all-building scan but does **not** finish
the tick-latency requirement. The existing
40,000-expansion limit is per route, with up to 64 nodes in a plan; it does not
bound a whole planning pass to a small tick allowance. Spreading that work
across ticks must account for its effect on action timing and replay hashes.

These are shared-machine synthetic measurements, with test-only timing
overhead; the final column also includes the passability counters below.
They do not reproduce the owner's entire save, establish the cause
of its 2,192 ms maximum, or qualify finished RTS performance or dedicated
hardware. The occupancy bound is structural; elapsed times are observations.

## Per-plan navigation reads

The remaining road search repeatedly queried the same ground and directed
edges. A cache now lives only inside one immutable `roads::sites` call, for
passability, free tiles, owned infrastructure and directed terrain edges.
Each query map stores values in a fixed arena of 65,536 booleans; misses with
no available storage read the world normally, without changing answers or
truncating the search. False answers
are cached too. No unit-dependent bridge validity survives a planning call.

The best-state table now uses a lookup-only hash map. It is never iterated;
the existing heap priority (estimated cost, path cost, y/x and direction)
still orders the search.
The original canonical hash assertion is unchanged and passes.

With additional terrain-read counters enabled on both sides of this change:

| Measurement, three replans | Before memoization | Memoized |
| --- | ---: | ---: |
| Explicit tile-passability reads | 303,621 | 27,840 |
| Maximum explicit passability reads of one tile in one plan | 19 | 1 |
| Bridge validations | 648 | 648 |
| Occupancy visits | 3,831 | 3,831 |
| Bridge validation total | 2.206 ms | 3.842 ms |
| Three Vassal passes | 231.506 ms | 186.866 ms |
| Worst Vassal pass | 86.974 ms | 76.490 ms |

These counters cover the planner's explicit tile-passability queries, excluding
tile reads inside edge and bridge validation. The new read bound failed before
memoization and passed afterward. The cache
capacity/false-answer regression also passed. These elapsed times are separate
shared-machine observations, with additional instrumentation; they are not a
dedicated-hardware qualification. The pass still exceeds approximately 5 ms.
An earlier focused arena measurement observed 65.604 ms for the worst pass;
the final sample above was taken after this checkout's full suite finished.

## Remaining work

Deterministic spreading across ticks is outstanding. The brief also requires
"hashes identical (no repin)". `vassal_plan` currently installs the entire plan
and its timestamp in the same AI tick; `hash_vassals` hashes that timestamp and
every plan site (`economy/vassal/plan.rs`, `economy/vassal/hash.rs`). A deferred
plan would expose different intermediate hashed state, and could delay builder
orders. Clarification is pending on whether exact timing/hashes must remain
identical, or deferred work may preserve completed plan decisions while changing
intermediate timing. This increment keeps the original timing and hashes.

## Correctness coverage

The occupancy oracle compares indexed overlap with the original exact
`contains_tile` predicate over many candidate angles. It exercises insertion,
index rebuilding and removal for foundations, completed roads, farms, walls,
gates and angled bridges. A separate placement test checks that a nonblocking
road or foundation on a bank rejects the bridge and that removal restores
validity. Existing bridge, Vassal and replay tests remain unchanged.

## Local checks

All commands use `CARGO_PROFILE_DEV_DEBUG=0` and Dockerized Make targets.
The indexed lookup increment (`fd9ed3cb670b2134ac5c0d5223805ac7a4d963d9`)
passed the following checks:

- `make hooks-install hooks-check fmt-check structure-check architecture-check`:
  passed.
- `make lint`: passed, including WASM test-target compilation; two warnings
  in untouched client presentation code remain.
- The focused reproduction above: all three tests passed. The baseline run
  failed only the intended occupancy-work bound.
- `NEXTEST_TEST_THREADS=2 make test-native-focused
  AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'`:
  1,227 passed, zero skipped, in 692.796 seconds. This includes Vassal and
  bridge tests, one-/four-worker replay checks and both real-river Vassal tests.

The memoized increment also passed:

- `make hooks-install hooks-check fmt-check lint structure-check
  architecture-check docs-check`.
- `NEXTEST_TEST_THREADS=2 make test-native-focused
  AOE_NATIVE_TEST_PACKAGE=aoe-simulation
  AOE_NATIVE_TEST_FILTER='test(vassal::scale::)|test(road_cache::)'`: two passed.
- `NEXTEST_TEST_THREADS=4 make test-native-focused
  AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'`:
  1,228 passed, zero skipped, in 623.146 seconds.
- `NEXTEST_TEST_THREADS=1 make test-native-focused
  AOE_NATIVE_TEST_PACKAGE=aoe-simulation
  AOE_NATIVE_TEST_FILTER='test(vassal::scale::)'`: final measurement passed.

No visible behavior change is intended in this indexed lookup increment, so
no before/after gameplay video was recorded. The owner's save and CI have not
been run. Publication is excluded by the task brief; no ship or push is run.

## Stop-check recovery

The first stop check after `36eacfbafbfef502d2a0b3433ca2cef6b579c55f`
hit the 180-second architecture-check budget during a cold harness build.
The other reported failures were immediate resource-admission refusals below
the 3 GiB available-memory floor. After memory recovered, the unchanged source
passed `make architecture-check` and then `make browser-check deny docs-check
fmt-check lint structure-check architecture-check`, using the default build
profile that the stop checks use. No gate, baseline or test was weakened.

Recovery logs are local ignored artifacts:
`.cache/cx-bridge/stop-architecture-recovery.log` and
`.cache/cx-bridge/stop-gates-recovery.log`. The original stop report remains in
the hook-managed `.cache/agent-hook/state.json`. Build artifacts are retained
through the successful stop rerun; final cleanup belongs at the end of the
unfinished goal. These local results do not qualify CI or dedicated hardware.
