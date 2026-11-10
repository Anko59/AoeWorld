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
restarted merely because its age or replan timestamp is stale. Completion
refreshes that timestamp. Removal of a domain drops its continuation.

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

The supervisor explicitly authorized timing-dependent hash changes. Only the
affected Vassal checkpoint is repinned, in
`crates/simulation/src/game_world/tests/economy/vassal/budgeted-checkpoint.txt`:
`9ad415012c52f1e713ddf354dd153670a4c86aecfd762c603cda27635641a15a`.
Its reason is: **Vassal plan work budgeted per tick, decisions land a few ticks
later.** The static-world road/bridge decision fixture remains exactly the
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
