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
| Above + two-way point memo (`ad2a51975`) | 260,977,583 | +32.10% |
| Bounded endpoint certificates | 245,005,616 | +24.02% |

The local 3% ceiling is **203,487,343** instructions. The repository's stored
CI comparison is 240,557,748; it is not the result of the fresh merge-base run.
The supplied CI HEAD result, 296,624,316, is close to the measured HEAD.
No baseline was repinned or relaxed.
That increment reduces original HEAD by 35,659,855 instructions (12.02%),
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

Static/hook checks pass on that increment. Lint retains the two
existing client presentation dead-code warnings. CI was not run. No push,
publication or performance-baseline change was made. Raw logs and Callgrind
profiles stay in ignored `.cache/region-perf/`, including the base, original
HEAD, repaired midpoint, intermediate, and current NDJSON counts.

## Bounded endpoint certificates

The next increment reuses region labels for the far planner's destination
pocket question too. An optional query visits at most 64 graph nodes, counting
cached nodes identically; exhaustion returns unknown and leaves the ordinary
resumable search intact. Negative answers also retain endpoint refinement and
the legitimate search. Clean labels from the dense census are reused, while
lazy and dense invalidations remain independently pending.

Positive connectivity certificates discharge the duplicate reverse flood and
survive continuation legs. A separate straight-path certificate can replace
early connectivity-only portal guidance. A graph connection alone retains
corridor guidance: suppressing it stranded two soldiers in the strict bridge
regression, so that prototype was rejected. The difficult-leg work threshold
still requests guidance even with a straight witness.

Certificates retain a shared collision snapshot. Obstacle removal and resource
depletion preserve positive witnesses; new obstacles, resource regrowth and
bridge edits invalidate them. Certified nearest endpoints retain the original
sealed-goal bounds, so opening that goal restarts the original order. Planner
proof state participates in the deterministic replay hash.

Region chunk interiors still use bulk terrain reads. Their one-tile halos use
sparse reads, reducing the cold halo regression from nine composed chunks to
one with identical directed topology. Portal construction also reuses its
already-required chunk projection for inside border points; the cold point-read
regression falls from 189 entries to at most 72. Cache byte caps are unchanged.

The first unbounded prototype measured 238,558,060 Ir (+20.75% base), before
the bounded query, continuation witness and certified endpoint refinements.
Its inclusive profile attributes 97.35M to route service, 34.21M to guidance,
30.38M to portal construction, 20.29M to point memoization and 5.96M to pocket
checking. These nested counts must not be summed. It is diagnostic evidence,
not qualification of the final source.

The guarded candidate measures **245,005,616 Ir**, 6.12% below `ad2a51975`
and 17.41% below original HEAD. It remains **41,518,273 above the 3% ceiling**;
the performance regression is not resolved. The smaller unbounded prototype
count does not qualify the guarded candidate's behavior.

| Inclusive stage | Fresh base | Guarded candidate |
| --- | ---: | ---: |
| Route service | 57,229,304 | 104,157,048 |
| Waiting-route search | 56,178,945 | 101,012,379 |
| Neighbor expansion | 17,140,008 | 50,740,724 |
| Guidance | 270,582 | 32,885,473 |
| Portal construction | 0 | 29,620,872 |
| Grid construction | 0 | 28,327,337 |
| Navigation cells | 0 | 23,428,051 |
| Pocket checking | 37,145,977 | 1,933,002 |

Nested rows must not be summed. Named compiled call edges show 79 planner
polls versus 165 at base, but 10,808 neighbor-expansion calls versus 4,324.
Two grid constructions consume 28.33M Ir; one portal construction consumes
29.62M. Region witness reuse costs 10.67M across 160 calls. These are Callgrind
call edges, not replacements for the logical work gates. The remaining
attribution is longer searches and expensive terrain projections, rather than
more polls or the old pocket proof. Region connectivity alone cannot safely
replace detour guidance; extracting usable corridor guidance from the labels
is a possible next step, requiring the unchanged bridge-arrival regression.

Raw evidence: `bounded-endpoint-final.ndjson`,
`bounded-endpoint-final.callgrind`, `bounded-endpoint-final-inclusive.txt` and
`route-call-attribution.json` in ignored `.cache/region-perf/`.

Qualification: all **421 map tests pass** (174.047 seconds). The full simulation
run passes 1,263/1,265 tests (743.010 seconds); its only failures are the old
replay hash and lower exact long-march work pin. After explained current-output
updates, **12 focused simulation tests pass** (228.942 seconds), including the
full-population checkpoint, unchanged activity floors and one/four-worker
equality, long march, river, strict three-river bridge and all region-route
cases. The final checkpoint is `a1c5fae0…`. Long-march arrival tick, collision
work, reversals, speed, membership and exact slots remain fixed; charged route
work falls from 378,134 to 313,361. River route work falls from 55,349 to 55,320
with its physical output pins fixed. No acceptance bound changes.

`hooks-install`, `hooks-check`, `fmt-check`, `lint`, `structure-check`,
`architecture-check` and `docs-check` pass. Lint retains the two existing client
presentation warnings. No CI, push or publication was run. The full simulation
suite was not repeated after those two output-pin updates; the focused rerun
above verifies both, with production source unchanged.

