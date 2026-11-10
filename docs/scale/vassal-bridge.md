# Vassal bridge occupancy

The bridge span placement predicate now looks up each raster cell in the
existing building footprint index. That index includes unfinished and
nonblocking structures, and indexes angled bridges by raster cell rather than
by their bounding rectangle. No new persistent index or cached placement
validity is introduced. Unit-dependent validation still runs for each candidate.

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

| Measurement | Original scan | Indexed occupancy |
| --- | ---: | ---: |
| Buildings | 10,002 | 10,002 |
| Bridge validations | 648 | 648 |
| Occupancy visits | 6,481,296 buildings | 3,831 tiles |
| Validation total | 648.237 ms | 4.563 ms |
| Three Vassal passes | 1,029.297 ms | 321.613 ms |
| Worst Vassal pass | 353.178 ms | 128.722 ms |

The new occupancy work bound failed against that implementation, as intended.
The captured pre-change canonical hash is pinned in the regression:
`bf86364882cff97061b1c6dcc97acb200da621250ffb65b590d97dfe5acf870e`.
No existing hash fixture has been repinned.

Both runs produced that same hash and the same complete road/bridge plan.
The indexed run passed the occupancy bound and both new correctness tests.
This removes the all-building scan but does **not** finish the tick-latency
requirement: a Vassal pass still exceeds approximately 5 ms. The existing
40,000-expansion limit is per route, with up to 64 nodes in a plan; it does not
bound a whole planning pass to a small tick allowance. Spreading that work
across ticks must account for its effect on action timing and replay hashes.

These are shared-machine synthetic measurements, with test-only timing
overhead. They do not reproduce the owner's entire save, establish the cause
of its 2,192 ms maximum, or qualify finished RTS performance or dedicated
hardware. The occupancy bound is structural; elapsed times are observations.

## Correctness coverage

The occupancy oracle compares indexed overlap with the original exact
`contains_tile` predicate over many candidate angles. It exercises insertion,
index rebuilding and removal for foundations, completed roads, farms, walls,
gates and angled bridges. A separate placement test checks that a nonblocking
road or foundation on a bank rejects the bridge and that removal restores
validity. Existing bridge, Vassal and replay tests remain unchanged.

## Local checks

All commands use `CARGO_PROFILE_DEV_DEBUG=0` and Dockerized Make targets.

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

No visible behavior change is intended in this indexed lookup increment, so
no before/after gameplay video was recorded. The owner's save and CI have not
been run. Publication is excluded by the task brief; no ship or push is run.
