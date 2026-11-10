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

The local 3% ceiling is **203,487,343** instructions. The repository's stored
CI comparison is 240,557,748; it is not the result of the fresh merge-base run.
The supplied CI HEAD result, 296,624,316, is close to the measured HEAD.
No baseline was repinned or relaxed.

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

| Stage | Measured base | Original HEAD |
| --- | ---: | ---: |
| Portal guidance | 270,582 | 65,714,982 |
| Portal graph construction | 0 | 58,448,914 |
| Destination-pocket check | 37,145,977 | 41,734,663 |
| Simulation land-reachability query | 0 | 15,533,138 |

The portal cache previously retained only one collision fingerprint per
chunk. Live searches pin older collision snapshots; alternating queries can
replace each other's graphs even below the byte limit. The cache now keys
entries by both chunk and collision fingerprint, preserving the existing
byte cap, LRU policy and graph construction. It changes no route verdict or
planner allowance. The patched kernel has not yet run: normal resource admission refused it
(exit 75, surfaced as Make exit 2) while another worktree held `make preflight`.
There is **no fixed-count or within-3% qualification yet**.

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

Exact focused commands (all with `NEXTEST_TEST_THREADS=2`, serial builds):

```sh
make -j1 test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-map AOE_NATIVE_TEST_FILTER='test(navigation::hierarchy::)'
make -j1 test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(late_game_full_population_replays_at_the_profile_checkpoint) | test(routes::regions::) | test(economy::regions::) | test(economy::gathering::crowd::) | test(economy::general::engage::)'
make -j1 hooks-install hooks-check fmt-check structure-check architecture-check docs-check
make -j1 lint
```

These checks pass. Lint retains the two existing client presentation dead-code
warnings. Full suites and CI were not run for this increment. No push,
publication, replay repin or performance-baseline change was made. Raw logs
and Callgrind profiles stay in ignored `.cache/region-perf/`.

