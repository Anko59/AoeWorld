# Parallel movement measurements

Companion to [scale measurements](measurements.md). Figures below were taken on branch
`perf/parallel-movement` at the fix commit on top of `a564a8d4`, rebased onto dev `e9c3f0b5`.
Earlier figures (`24c29d9a` and before) are superseded.

These measurements predate the playtest bundle's fair route scheduler and spacing;
they were not repeated for that integration. In the current engine the fair
per-player search pass runs before movement, and lookahead uses its remaining
budget afterward. A mover needing a search always waits in the movement loop,
even with free budget or slots. Proposals preserve that wait and its start tick,
and recheck follower spacing during ordered application. The replay pins were
regenerated from the merged sequential loop and verified with proposals enabled
and with 1, 2 and 6 threads. The scheduler's fast attempts, sealed proofs, near/far
slots and planner retention remain in place.

Same workload and command (`make perf-scale`, 100 players, 101,111 units, 600 ticks fog
on, 100 fog off). "Sequential" is the same tree with the proposal phase disabled (every
mover takes the sequential path, vision still batched); its hashes equal the parallel runs'.
"Before fix" is `a564a8d4`. Raw reports are in `.cache/` (not committed). The container has
6 CPUs and the machine was shared (1-minute load 11 to 14), so absolute numbers carry noise:
each cell is p50 / p95 in ms over the runs listed.

| Milliseconds, p50 / p95 | sequential (2 runs) | before fix (2 runs) | after fix (3 quiet runs) |
|---|---|---|---|
| Movement, fog on | 35.7-36.2 / 39.4-51.1 | 22.3-25.1 / 56.3-63.2 | 17.9-20.0 / 20.9-35.2 |
| Whole `advance`, fog on | 48.8-49.0 / 53.4-68.7 | 38.3-41.2 / 72.9-81.2 | 30.9-34.7 / 35.8-57.2 |
| Movement, fog off | 33.3-33.5 / 36.1-36.5 | 20.7-20.8 / 24.7-25.0 | 16.9-19.9 / 18.7-22.5 |
| Whole `advance`, fog off | 45.5-46.0 / 48.8-49.6 | 35.2-35.4 / 40.1-41.5 | 29.5-33.5 / 32.6-38.2 |

A fourth after-fix run was taken while another job loaded the machine (fog off also doubled:
movement 35.0 / 52.9) and is excluded as contention, not a measurement of the code. Its hashes are
identical like all others.

**Root cause of the fog-on p95 regression (measured).** A per-tick timer inside
`advance_movement` (proposal, sequential apply, sequential fallbacks, vision flush) and a
counter of why proposals returned `None` showed that in 258 of 600 fog-on ticks (from tick 228)
about 70,000 of 70,000 movers took the sequential fallback (`proposed` 0 to 17), costing
proposal 9 ms plus fallback 45 to 54 ms, worse than purely sequential. The reason counter put
all of them on one cause: a mover that has reached the end of its segment and needs a route
search. The proposal assumed "wait" only when planner capacity was already exhausted at the
start of the tick, but the sequential engine also waits once the tick's planning budget is
spent, and that is the common case: after the first few dozen movers in rotation order take
the 64 planner slots, all the thousands of movers behind them only wait. Planner capacity is
free at the start of the tick, so the proposals were all discarded. Vision, rayon start-up,
buffer allocation (dense tables 0.8 ms) and uneven chunks were not the cause: vision flush was
0.01 ms on average, and fog-off ticks (which never hit the case) showed the p95 within 2 ms of p50.

**Fix.** A mover that needs a search is now proposed as waiting, and `apply_step` confirms the
sequential engine's own condition at that mover's turn (`planning_budget == 0` or planner
capacity exhausted); otherwise it runs the sequential code, so the budget is spent exactly as
before. The hashes of all runs equal the sequential hashes.

The goal of a movement p95 under 10 ms is **not reached**: after the fix movement p50 is
about 18 to 20 ms (fog on) and p95 21 to 35 ms, so the remaining distance to 10 ms is about 2x at
p50 and 2 to 3.5x at p95. Fog-on p95 is now no worse than sequential (20.9 to 35.2 against 39.4 to
51.1) and p50 is 1.8 to 2 times lower. Movement is about 7 ms proposal (parallel), 10 to 11 ms
sequential apply (it was measured at 4 ms before the rebase; the rebase raised the movers from
43,665 to 69,480), and a few ms of route search fallbacks. The apply phase is now the largest
piece and is write-bound on `StoredUnit`; shrinking it (structure of arrays, parallel apply of
disjoint writes) is the next step, not done here.

**Determinism.** Initial and all 6 fog-on and 1 fog-off canonical hashes are identical in every
run of sequential, before-fix and after-fix (python comparison of the reports), and dev's gathering
and wildlife replay pins hold unchanged. The tests in
`game_world/tests/physical_movement/parallel.rs` replay 1,800 mixed units for 160 ticks on 1, 2
and 6 rayon threads and require identical canonical hashes and change sets, once without buildings and once with
three town centres in sight of the movers (so the batched building-watch events run, and each
player's remembered foreign buildings are part of the digest). Both pin the final hash and a digest of
all changes, produced by the sequential engine (proposal phase disabled) and unchanged by the fix.

**What changed.**

1. *Proposal and apply* (`game_world/movement_step/mod.rs`). A pure `&self` proposal per mover runs
   under rayon (rayon splits the rotation-ordered mover range with a minimum chunk of 512; proposals are
   pure, so the split never changes a result) and computes the unit's whole tick when it needs no shared planning state:
   a step inside the segment, an arrival that takes the next route tile, a stop at the final
   tile, and a wait for planner capacity. A sequential apply then performs the writes in the
   planning-cursor rotation order of the old loop. A proposal that assumed the planner
   capacity was exhausted re-checks it at apply time, and any other case (a search, a failure,
   two waypoints in one tick, a planner that wants lookahead) runs the unchanged sequential
   code in the same order, so the shared planning budget is spent exactly as before. Before the rebase
   43,663 of 43,665 movers were proposed per tick; the share after it was not re-counted.
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

**Route searches.** The searches of the at most 64 active planners (up to 16,384 work units
per tick) still run in the sequential fallback because their planning budget is shared state.
Parallelising them exactly needs speculative polling in windows no larger than the remaining
pool, validated at apply time; not done.

**Follow-ups from the #552 review.**

- F1: `far_attack_destination` skips candidate tiles that have no centre (west and north
  edges) instead of failing the whole order.
- F2: the portal graph treats resource-node tiles as open, so `Unreachable` stays a proof
  when a node is later depleted.
- F3: a test seals an attacker in a water-ringed pocket and requires the rejection.
- F8, F10, F12: a far firing tile is cached per attacker while the target keeps its tile and
  the attacker is more than 64 tiles away; far searches (first ones, for a target more than 64 tiles away, and repeats) get at most 4 per
  tick and the others retry next tick; at most 16 separate groups of candidate tiles are
  proven over the portal graph per search. A tile proven unreachable condemns the candidates
  joined to it by clear steps, so an islet costs one proof, and past the cap the next candidate
  is walked to instead of rejecting the order.
- F9: a run of open border pairs is split where a height step or wall separates neighbours,
  so each part keeps its portal.
- F11, F13: the portal cache is bounded to 64 MiB with least-recently-used eviction; a chunk
  is charged for its full cost matrix.

**Limits.** One run per configuration; the machine was not idle. Route search work and the
sequential apply remain on one thread. The pinned small-world test shares the code path with
the benchmark but not its scale.
