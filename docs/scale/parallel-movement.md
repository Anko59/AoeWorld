# Parallel movement measurements

Companion to [scale measurements](measurements.md). The figures below were taken on
branch `perf/parallel-movement` at `1beec88b`, before the rebase onto dev after #558
(`2adb4339` is the rebased revision). They have not been re-run on the rebased code.


Same workload and command (`make perf-scale`, 100 players, 101,111 units, 600 ticks fog
on, 100 fog off). Raw reports: `reports/scale/before-par.json` and
`reports/scale/after-par.json` (not committed, `reports/` is ignored). The container has 6
CPUs and the machine was shared with other agents' builds (1-minute load 4 to 12), so the
absolute numbers carry noise; the ratio and the hashes are the result.

| Milliseconds | before p50 | before p95 | after p50 | after p95 |
|---|---|---|---|---|
| Movement and pathfinding, fog on | 30.5 | 41.9 | **12.5** | **28.8** |
| Whole `advance`, fog on | 115.7 | 164.8 | 90.6 | 103.5 |
| Movement, fog off | 26.2 | 37.3 | 10.1 | 25.3 |
| Whole `advance`, fog off | 89.1 | 130.3 | 64.7 | 80.7 |

The goal of a movement p95 under 10 ms is **not reached**: p50 is 2.4 times lower and p95 1.5
times lower. Wildlife (48 ms p50), building memory (21 ms) and gathering (12 ms) are now the
larger phases of the tick.

**Determinism.** All 6 fog-on and 1 fog-off canonical hashes and the initial hash are
identical before and after (`python3` comparison of the two reports). The test
`game_world/tests/parallel_movement.rs` replays 1,800 mixed units for 160 ticks on 1, 2 and 6
rayon threads and requires identical canonical hashes and identical per-tick change sets; it
also pins the final hash and a digest of all changes, produced by the sequential engine
before the proposal phase existed.

**What changed.**

1. *Proposal and apply* (`game_world/movement_step.rs`). A pure `&self` proposal per mover runs
   under rayon (fixed shards of 512 by position in the sorted mover list, so the split never
   changes a result) and computes the unit's whole tick when it needs no shared planning state:
   a step inside the segment, an arrival that takes the next route tile, a stop at the final
   tile, and a wait for planner capacity. A sequential apply then performs the writes in the
   planning-cursor rotation order of the old loop. A proposal that assumed the planner
   capacity was exhausted re-checks it at apply time, and any other case (a search, a failure,
   two waypoints in one tick, a planner that wants lookahead) runs the unchanged sequential
   code in the same order, so the shared planning budget is spent exactly as before. At the
   benchmark 43,663 of 43,665 movers are proposed per tick.
2. *Reads made lock free.* The proposal uses per-tick dense tables (owner economy by player,
   a bitset of units with a gather, hunt, attack or build job) and a dense observer table
   (`visibility/observers.rs`) instead of ordered-map lookups. The walk cache keeps four
   recent chunks per thread (`terrain/walk_cache.rs`), keyed by a process-unique generator
   id, so parallel readers skip the cache lock and the shared reference count; this alone
   took the serial proposal from 15 to 11.6 ms and the parallel one from 10 to 5 ms.
3. *Vision* (`visibility/batch.rs`). A player's sight is touched only by that player's
   observers and every update is a commutative count or set, so movement queues observer
   changes and applies them per player in parallel afterwards. At this workload few units
   cross a tile per tick, so the measured gain is small; it matters when many do.
4. Structure of arrays was not done: the proposal reads `StoredUnit` once per mover and the
   apply phase (about 4 ms) is dominated by writing it back, which a SoA would not remove.

**Where the rest goes.** Measured with a per-phase timer inside `advance_movement` on a loaded
machine: proposal about 5 ms (parallel; 11.6 ms serial), sequential apply about 4 ms, change
sort 0.5 ms. The p95 above p50 is **not diagnosed**; the likely cause (not measured) is the
route searches of the at most 64 active planners, up to 16,384 work units per tick, which run
in the sequential fallback because their planning budget is shared state. Parallelising them
exactly needs speculative polling in windows no larger than the remaining pool, validated at
apply time.

**Follow-ups from the #552 review.**

- F1: `far_attack_destination` skips candidate tiles that have no centre (west and north
  edges) instead of failing the whole order.
- F2: the portal graph treats resource-node tiles as open, so `Unreachable` stays a proof
  when a node is later depleted.
- F3: a test seals an attacker in a water-ringed pocket and requires the rejection.
- F8, F10, F12: a far firing tile is cached per attacker while the target keeps its tile and
  the attacker is more than 64 tiles away; attackers that already needed the far fallback get
  at most 4 full searches per tick (the others retry next tick); at most 16 candidate tiles
  are proven over the portal graph per search.
- F9: a run of open border pairs is split where a height step or wall separates neighbours,
  so each part keeps its portal.
- F11, F13: the portal cache is bounded to 64 MiB with least-recently-used eviction; a chunk
  is charged for its full cost matrix.

**Limits.** One run per configuration; the machine was not idle. Route search work and the
sequential apply remain on one thread. The pinned small-world test shares the code path with
the benchmark but not its scale.
