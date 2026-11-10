# Vassal road and bridge planning

Bridge validation looks up each raster span cell in the existing footprint
index, including foundations, nonblocking roads, gates and angled bridge cells.
Road planning collects bridges and enemy Town Centers through the player/kind
indexes instead of scanning unrelated buildings.

## Deterministic planning across ticks

The road search is an owned continuation. Each world tick polls one pending
Vassal in entity-ID round-robin order. Each poll performs at most 128 operations:
heap pops (including stale pops), traceback edges, road tiles or bridge sites.
The 40,000 settled-state search limit, 64-node limit, costs and heap tie order
are unchanged. Yielding never discards a frontier or truncates a route.

Fortification sites remain usable while roads are pending. On a replan, the
previous network remains usable until its replacement finishes. A domain with
pending roads is not reported as developed, and a pending plan is never
restarted merely because its age or replan timestamp is stale. Explicit
replan requests have a separate earliest deadline: a changed threat side
requests an immediate rebuild, and a refused placement or stalled foundation
requests one within 200 ticks. Completion refreshes the ordinary cache timestamp
without postponing that deadline. A due request rebuilds fortifications while
retaining the current network and any in-flight road continuation. Builders may
already be laying that network; restarting it here could choose a competing
bridge span. The next ordinary network replan incorporates the updated sites.
Defence keeps the existing
fortifications available until that rebuild. Removal of a domain drops its
continuation.

Terrain, occupancy and directed-edge memoization is local to one immutable
poll. The fixed value arenas retain at most 65,536 answers per query map;
storage misses read the live world normally. Unit-dependent bridge validation
is checked again in later polls, and ordinary build orders validate placement
again before charging or inserting a foundation.

Pending state is included in the canonical hash. Ordered job data, traceback
and output are hashed canonically; a deterministic trace of frontier insertions
and pops hashes the lookup-only A* state without iterating its hash table.
Tests compare pending progress and complete one-/four-worker replays.

## Reproduction and measurements

All commands below use Dockerized Make targets. The prepared 256-tile river
fixture has one garrisoned Vassal, a Castle and Town Center on opposite banks,
and 10,000 remote road foundations. No builders change its planning input.
The benchmark forces three replans and measures every AI invocation through
completion, including continuation ticks. It also compares the complete ordered
road/bridge output against the original static-world plan.

```sh
NEXTEST_TEST_THREADS=1 make test-native-focused \
  AOE_NATIVE_TEST_PACKAGE=aoe-simulation \
  AOE_NATIVE_TEST_FILTER='test(vassal::scale::)'
```

Results are written to ignored `.cache/cx-bridge/measurement.txt`; the retained
samples are `before.txt`, `indexed.txt`, `final.txt` and
`budgeted-measurement.txt` in that directory.

| Measurement, three complete plans | Original scan | Indexed + memoized | Budgeted ticks |
| --- | ---: | ---: | ---: |
| Buildings | 10,002 | 10,002 | 10,002 |
| Measured AI calls | 3 | 3 | 642 |
| Bridge validations | 648 | 648 | 1,455 |
| Occupancy visits | 6,481,296 buildings | 3,831 tiles | 8,565 tiles |
| Validation total | 648.237 ms | 3.842 ms | 6.211 ms |
| Total AI work | 1,029.297 ms | 186.866 ms | 285.497 ms |
| Mean per measured AI tick | 343.099 ms | 62.288667 ms | 0.444699 ms |
| Worst measured AI tick | 353.178 ms | 76.490 ms | 1.397 ms |

The budgeted sample's worst creation call was 0.724 ms, worst continuation
call 1.397 ms, and maximum observed poll work exactly 128 operations. Every
plan finished in 213 continuation ticks; none was abandoned. Rechecking live
validity across ticks increases aggregate work while reducing the tick stall.
The earlier occupancy-only increment observed a 128.722 ms worst pass.

These are shared-machine synthetic observations, with test-only counters and
clock overhead. The mean covers active measured planning calls, not all idle
world ticks between replans. The owner's full save and 99 simultaneous AIs
were not replayed. This does not establish the cause of the owner's 2,192 ms
maximum, qualify finished RTS performance or qualify dedicated hardware.

## Checkpoints and correctness

The Vassal checkpoint pins the current canonical state in
`crates/simulation/src/game_world/tests/economy/vassal/budgeted-checkpoint.txt`:
`fec0c5fc80da10927a55e0164854d7be916fb51cf299a007bb3c50853ba4ecc1`.
Its current reason is: **Vassal explicit replan deadlines are included in
canonical state.** The static-world road/bridge decision fixture remains exactly the
original ordered output. Search costs and route expansion limits were not lowered.

Correctness coverage includes:

- Original exact-footprint/index overlap oracles across insertion, rebuilding
  and removal, including foundations, roads, gates and angled bridges.
- Three competing domains yielding and completing without starvation.
- Deterministic pending-progress hashing, plus one-/four-worker Vassal replay.
- The original ordered river network, live placement checks and actual bridge
  building/crossing tests.
- Poll-operation and occupancy-visit bounds; false cache answers and arena
  exhaustion still return the correct live answer.

## Checks and limits

The pre-budget revision `36eacfbafbfef502d2a0b3433ca2cef6b579c55f` passed all
1,228 simulation tests and the static gates. The budgeted change's local
commands are:

```sh
make hooks-install hooks-check fmt-check lint structure-check architecture-check browser-check deny docs-check
NEXTEST_TEST_THREADS=4 make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'
```

The initial yielding regression failed before the continuation API existed.
The first budgeted Vassal run passed 43 of 44 cases, failing only its marked
checkpoint. At 256 operations per poll a focused sample reached 5.020 ms;
128 operations provide the measured margin shown above. Only the affected
checkpoint was regenerated; no gate, baseline or search budget was weakened.

The repository permits performance and AI-decision changes with nothing new
to show in-game to ship without video (`docs/showcase.md`, “A change with
nothing to see in the game”). Accordingly this delivery carries measured
numbers and replay tests rather than a gameplay recording. CI and the owner's
save have not been run. The brief excludes publication: no ship or push is run.

A previous stop check exhausted its 180-second cold-build budget and hit the
3 GiB memory-admission floor. Every failed target subsequently passed unchanged.
The recovery is recorded by `ac7a9e7c9cf95452729e1277e7934e484416b554` and
local `stop-architecture-recovery.log` / `stop-gates-recovery.log`. Build caches
are retained through the final stop check to avoid repeating that cold build.

## PR #750 review fixes F1–F3

`tests/economy/vassal/replan.rs` covers a staffed foundation that stalls (F1),
a raid from a new side (F2), and a refused building placement (F3), each while
the road job is pending. All three failed against `b7aae95c0995` before the
production fix. They now pass, including relocation of the refused foundation
and building site. A fourth test checks earliest-deadline coalescing, distinct
deadline hashes, and rebuilding during a still-pending search before tick 1,200.
Finished buildings remain reusable even when their old foundation was refused.

The same 10,002-building benchmark retains the original ordered network and
identical work counts: 642 calls, 81,630 operations, 1,455 bridge validations,
8,565 occupancy visits and a maximum of 128 operations per poll.

| Fresh sample | Mean AI call (ms) | Worst AI call (ms) | Total AI work (ms) |
| --- | ---: | ---: | ---: |
| Before fixes | 0.385781 | 1.390 | 247.671 |
| First after sample | 0.606811 | 4.131 | 389.572 |
| Identical after recheck | 0.471560 | 1.567 | 302.741 |
| Final network-retention fix | 0.306828 | 0.979 | 196.984 |

All samples are retained in `.cache/cx-bridge/brfix-*-measurement.txt`;
the first after run failed only the intentionally changed canonical checkpoint.
The recheck passed after regenerating that checkpoint for the new deadline
field. These shared-machine wall times vary; their cause was not measured.
No operation budget, search cost, activity floor or network fixture was reduced.

The first full simulation run passed 1,238/1,239 tests and exposed adjacent
bridges in the real-river Vassal test. Explicit fortification requests now retain
the existing network as well as its continuation; they do not start competing
network jobs while builders are still laying the retained spans. The unchanged
real-river assertions, review regressions and scale fixture passed together
(7/7 tests) with `AOE_NATIVE_TEST_FILTER='test(replan_tests::)|test(vassal_river::)|test(vassal::scale::)'`.
The final dedicated benchmark above ran after that fix, before the full rerun.

The requested before/after recordings show the real regression transcripts,
explicitly labelled as simulation-test evidence rather than gameplay. Both were
made with `make showcase SHOWCASE_STORYBOARD=.cache/showcase/brfix-<side>.json
SHOWCASE_OUT=.cache/showcase/brfix-<side>.webm SHOWCASE_LEVEL=medium`, checked
with `make video-probe SHIP_VIDEO=<path>`, and visually inspected at seconds 13
(before) and 16 (after).
Both are VP9, 1280×720, 25 fps. Recordings and storyboards stay outside Git:

- `.cache/showcase/brfix-before.webm`: 15.60 s;
  SHA-256 `9d0547370f852fcd69d1d49d3e4d06d6a8a479fc137ec38d33f8bbea6006ffaf`.
- `.cache/showcase/brfix-after.webm`: 17.92 s, refreshed seven-test take;
  SHA-256 `5f83308ee5101271560fb62c122f23b133720e66fa6160aa11ebc0e13ebd3a09`.

Local command logs use `.cache/cx-bridge/brfix-*.log`. The review regression
command selects `AOE_NATIVE_TEST_FILTER='test(replan_tests::)'`; the benchmark
selects `test(vassal::scale::)`; the full simulation run selects `all()`.
Each uses `NEXTEST_TEST_THREADS=2 make test-native-focused
AOE_NATIVE_TEST_PACKAGE=aoe-simulation`. No owner-save replay, gameplay capture,
CI run or dedicated-hardware qualification is claimed. Publication remains
excluded by the brief; these local videos are ready for a later `SHIP_VIDEO`.

The review-fix static run passed `make hooks-install hooks-check fmt-check lint
structure-check architecture-check browser-check deny docs-check` (native and
wasm Clippy included). `economy.rs` remains exactly 500 lines. The full simulation
result and exact committed revision are reported at handoff; a running test
process is not a pass.

The final refresh initially refused admission (Make exit 76) below the 3 GiB
memory floor. The full-suite Make launcher later received SIGTERM (exit 143);
its original Docker container remained running, with `OOMKilled=false`.
Its exit status and log were attached separately rather than restarting a live
workload: `.cache/cx-bridge/brfix-final-container-exit.txt` and
`brfix-final-container.log`. Those observations do not turn Make's exit 143
into a successful command. Final recovery results are reported at handoff.
The later bounded static/probe retry passed every target unchanged, including
the refreshed video's VP9 metadata check.
