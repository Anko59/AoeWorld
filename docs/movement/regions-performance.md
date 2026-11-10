# Region instruction regression (#743)

The comparison uses merge base `154862c455dfcef07f587c18d140422b7d5673ad`
and original branch HEAD `8f4ec2ac5fb826f5a1984a2f251c76073941960b`.
These are shared-host synthetic instruction measurements, not finished RTS
performance or dedicated-hardware qualification.

## Method and results

Run serially with the normal 4 GiB memory/swap and four-job container caps:

```sh
NEXTEST_TEST_THREADS=2 make -j1 perf-late-game-instructions
```

The focused target uses the same kernel and setup as
`instructions::simulation::late_game`: 100 seats, 14,000 initial units,
one current-thread Rayon worker, 384 warmup ticks, then the entire next
`world.advance()` under Callgrind 3.19 / Gungraun 0.19.4. The workload,
activity floors, compiler profile and performance baseline files are unchanged.
Diagnostic output is retained under ignored `.cache/region-perf/`.

| Source | Instructions | Change from measured base |
| --- | ---: | ---: |
| merge base `154862c455` | 197,560,528 | +0.00% |
| original HEAD `8f4ec2ac5` | 296,637,438 | +50.15% |
| `602e814aa` + existing `098852aca` build repair | 317,894,997 | +60.91% |
| Portal cache + near-route proof reuse | 271,989,950 | +37.67% |
| Above + sparse region point reads (`6701a7a29`) | 265,865,368 | +34.57% |
| Above + two-way point memo (current source) | 260,977,583 | +32.10% |

The local 3% ceiling is **203,487,343** instructions. The repository's stored
CI comparison is 240,557,748; it is not the result of the fresh merge-base run.
The supplied CI HEAD result, 296,624,316, is close to the measured HEAD.
No baseline was repinned or relaxed.
The current source reduces original HEAD by 35,659,855 instructions (12.02%),
but **does not meet the 3% target**. Its count still exceeds that ceiling by
57,490,240 instructions. The dependent frames work is not qualified by this
partial improvement.

## Bisect limits

`git bisect start 8f4ec2ac5 154862c455` selected `afdc46e72` first.
Its existing build fails at `deadline.rs`: `IdSet` has no `binary_search`.
The branch already repairs that code in `098852aca`. For intermediate
measurements only, that exact existing cleanup was applied as a diagnostic
patch; no fix to an older revision was committed.

The repaired `602e814aa` is above the ceiling. The next point, repaired
`3e3f90734`, produced no count after more than thirteen minutes of Callgrind
setup/warmup and was stopped by its owned container ID. No partial count is
accepted. The earlier revisions share the unbuildable call and the eager
whole-map fallback. The remaining range is `32e731f87`, `eb77242f3`,
`3e3f90734`, `afdc46e72`, `602e814aa`: this bisect does **not** identify one
first offending commit. `bisect.log` and the diagnostic patch preserve that
limit. Certified-endpoint preservation is not the first regression: its
predecessor measurement is already worse than HEAD.

## Profile and cache change

Inclusive counts (nested rows must not be summed):

| Stage | Measured base | Original HEAD | `6701a7a29` |
| --- | ---: | ---: | ---: |
| Portal guidance | 270,582 | 65,714,982 | 54,211,135 |
| Portal graph construction | 0 | 58,448,914 | 38,863,076 |
| Destination-pocket check | 37,145,977 | 41,734,663 | 45,999,122 |
| Simulation land-reachability query | 0 | 15,533,138 | 6,369,806 |

The portal cache previously retained only one collision fingerprint per
chunk. Live searches pin older collision snapshots; alternating queries can
replace each other's graphs even below the byte limit. The cache now keys
entries by both chunk and collision fingerprint, preserving the existing
byte cap, LRU policy and graph construction. It changes no route verdict or
planner allowance. This cache-only increment is committed as
`bc59b10933796d0833f319ccee8e80d9a6c57b48`. The first attempted measurement
was refused by normal resource admission while another worktree held
`make preflight`; subsequent measurements above ran after the slot was free.

The new regression fails before the change: querying a retained fingerprint
after inserting the current one loses the original graph. The red run is on
`154862c455`; the cache implementation is byte-identical at original HEAD.
Existing preparation tests retain their assertions with updated internal
value destructuring. After the change, **39 hierarchy tests pass**. The
focused simulation run passes **19 tests**, including the 14,000-unit
one/four-worker replay, crowd gathering, certified-route rejection/retention,
local cold orders, gates, and the five-deployment General engagement gate.
The replay pin remains
`2a0848c5c015012cd58c02c48a62c0849bca08e16a8db429275857df1aba9730`.

## Proof and point-read increment (`6701a7a29`)

A successful current directed region query now discharges the near planner's
duplicate reverse pocket flood, only when both the current leg origin and
effective destination match. Disconnected searches and certified replacement
endpoints keep their existing handling. The regression previously observed
eight pocket-flood entries; it now observes zero while the detour remains live
with valid route accounting. Far-route pocket checks are unchanged and still
dominate the remaining pocket cost.

`MapRegionGround` now uses local point reads for sparse tile/edge queries.
Dense region construction retains its bounded nine-chunk bulk path. A new
regression fails before this change because one sparse tile read composes a
whole walk chunk. Cold provider failures still return an error and retry after
recovery; an already cached immutable witness remains usable during provider
unavailability. The recovery fixture now clears setup's point-cache warming so
it actually tests a failed source read, with its original assertions intact.

The replay pin changes from `2a0848c5…` to `eeb9a171…`: removing duplicate
charged proof work lets planners spend their allowance on routes sooner.
The unchanged activity floors and complete one/four-worker equality pass.
No workload, threshold, performance baseline, or cache byte limit changes.

After the point-read change, 54 focused map tests pass, including hierarchy,
region topology equivalence, cancellation, bounded pocket work, and the distant
river detour. Five focused region simulation tests pass after the fixture
correction. The full-population replay also passes (194.782 seconds). The
earlier full simulation run passed 1,260 of 1,261 tests; its sole failure was
the old replay pin, before the explained repin above. The full run at this
increment passes
**all 1,262 simulation tests** (567.455 seconds, zero skipped), including the
one/four-worker checkpoint and army bridge, river, recovery and General gates.
The full map suite also passes **all 404 tests** (158.042 seconds, zero skipped).

## Two-way point memo

Composed-point reads remain a substantial cost after localizing sparse queries.
The fixed native point/base tables now use two-entry sets instead of replacing
each colliding entry immediately. Slot counts and retained byte budgets are
unchanged; the browser still allocates no table. Immutable results, cancellation
and source-error handling are unchanged. The collision regression fails before
the change; it checks both tables' retained pair, fixed capacity and subsequent
eviction. The implementation retains insertion order within each set.

The same instruction target measures 260,977,583, 1.84% below `6701a7a29`.
All **405 map tests** pass (152.143 seconds), as do **21 focused simulation
tests**, including the unchanged full-population checkpoint (187.698 seconds),
one/four-worker equality, crowd gathering, General engagement, regions and
certified endpoints. Static/hook checks pass. No further replay repin was
needed. The 1,262-test full simulation run above predates this cache-only change;
it was not repeated. The saved count is `point-associative-working.ndjson`.

Exact focused commands (all with `NEXTEST_TEST_THREADS=2`, serial builds):

```sh
make -j1 test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-map AOE_NATIVE_TEST_FILTER='test(navigation::hierarchy::)'
make -j1 test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(late_game_full_population_replays_at_the_profile_checkpoint) | test(routes::regions::) | test(economy::regions::) | test(economy::gathering::crowd::) | test(economy::general::engage::)'
make -j1 test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-map AOE_NATIVE_TEST_FILTER='all()'
make -j1 test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'
make -j1 hooks-install hooks-check fmt-check structure-check architecture-check docs-check
make -j1 lint
```

Static/hook checks pass on the current production source. Lint retains the two
existing client presentation dead-code warnings. CI was not run. No push,
publication or performance-baseline change was made. Raw logs and Callgrind
profiles stay in ignored `.cache/region-perf/`, including the base, original
HEAD, repaired midpoint, intermediate, and current NDJSON counts.

