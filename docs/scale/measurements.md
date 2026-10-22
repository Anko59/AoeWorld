# Scale measurements: 100 players, 100,000 units, 16,000-tile map

Status: measured, not solved. `make perf-scale` builds the workload below and
writes `reports/scale/perf-scale.json` (raw compact JSON; the committed copies are
`reports/scale/perf-scale.json`, `perf-scale-idle-server.json` and the 20-player
before/after pair). Measured on the 12-core development machine (heavy slot: 6 CPUs
for the container, the simulation tick itself is single-threaded), release profile,
revision 54fda3bb plus the changes in this branch.

## Verdict

The promised scale does not run in real time today. On this machine one
authoritative tick at 100 players and 101,111 units takes **381 ms p50 / 421 ms p95 /
797 ms max against a 50 ms budget (20 Hz)**: 7.6 times over, and every one of 600
measured ticks missed. The server adds **about 115 ms per tick for 100 subscribed
sessions** on top. Memory is not the problem (about 100 MiB resident for the world,
about 200 MiB with the server fan-out).

It is reachable on 12 cores, but only with architecture changes, not tuning: a
simulation that is about 10 times cheaper per mover (chunk-level terrain cache,
hierarchical pathfinding, dense unit storage), parallel execution with a
deterministic merge, and an interest-managed, per-player server fan-out that runs
off the tick thread. Section "What reaching the target takes" gives a budget. The
dominant cost, movement (294 of 381 ms), is embarrassingly parallel and has a known
4 times algorithmic saving, so a 40 to 60 ms tick is plausible, not certain.

Before any of that, several rule caps make the target unreachable independent of
speed (see "Hard limits"): 16 players, 80 gatherers world-wide and 200 population
per player (100 players x 200 = 20,000 units, not 100,000).

## What was measured

Workload (`crates/harness/src/perf_scale/`):

- Map: a procedural `MapPackage` (no source layers), France-centred
  (46.6 N, 2.5 E), compression 30:1, 2 m tiles, 16,000 x 16,000 tiles. A real
  source-backed France package was not generated: no source cache is available to
  this task, and page-backed terrain has a different cost profile (see Limits).
- 100 players on a 10 x 10 grid, 1,600 tiles apart. Each has a Town Center
  (`initialize_village`), up to 12 house foundations built by 3 villagers each, 900
  villagers and 100 militia (about 1,000 units, 101,111 in total with 968 fight units
  at the 50 neighbour midpoints and 100 scouts).
- About 41,400 villagers are assigned to gather (wood, food, gold, stone nodes within
  the village search radius); 30 of 100 bases found no wood in range on this
  synthetic map and 1,543 assignments were rejected, so about 48,000 villagers never
  got a job and stay idle. The rest walk, gather and return. 41,486 units were moving at tick 600.
- Orders: a deterministic driver stands in for 99 AI controllers and one human,
  3 orders per player per second (15 per tick): 35% gather, 25% military move,
  10% attack, 10% build, 10% train, 10% villager move. 10,500 were issued over 600
  ticks; outcomes are in the JSON (`orders_ok`, `orders_rejected`).
- 600 ticks (30 s of game time) with fog of war on, then 100 more with fog off to
  isolate visibility. Per tick: orders, then `advance`, split by phase with
  `GameWorld::advance_with_probe` (the simulation stays clock-free; the harness
  stamps phases). The replay hash is timed every 100 ticks.
- Server: the real `GameplayService` (`from_scale_world`) with 100 registered,
  subscribed sessions (96 x 96 tile views over each base), 40 ticks per mode, real
  `encode_gameplay_server` byte counts. Baseline ticks without sessions are measured
  first so fan-out is the difference.
- The shipped capacity caps were raised for the run with the new
  `GameWorld::set_limits` (defaults unchanged).

## Numbers (100 players, 101,111 units, fog on)

Simulation `advance`, 600 ticks, milliseconds:

| Phase | p50 | p95 | max |
|---|---|---|---|
| Movement and pathfinding | 293.9 | 325.6 | 682.0 |
| Wildlife | 42.5 | 48.1 | 61.4 |
| Gathering and economy | 17.4 | 19.8 | 27.7 |
| Building memory (fog, per player) | 17.6 | 19.5 | 31.3 |
| Result collation | 8.9 | 10.0 | 12.0 |
| Construction, combat, research, training | 0.5 | 1.7 | 4.9 |
| **Whole `advance`** | **381.3** | **421.0** | **797.1** |
| Orders issued (15 per tick) | 2.6 | 7.1 | - |
| Whole tick with orders | 384.9 | 423.2 | 798.5 |

- Fog off (100 ticks): `advance` 347 p50, movement 278, building memory 0. Fog on
  costs about 34 ms p50 here (17.6 building memory plus about 16 in movement
  vision updates). It grows with buildings and moving units.
- Replay hash: 47 ms per `canonical_hash` call (single-threaded, all units, routes,
  planners and vision). It is not called per tick by the server today.
- Idle world (all units idle, no orders): `advance` still takes 47 ms, entirely the
  wildlife scan.
- Setup: 15.6 s to build the world (5.4 s of it resource search).
- Memory: 7 MiB before, 93 MiB after build, 103 MiB after 600 ticks; 204 MiB peak
  with 100 server sessions.
- Changed units per tick: 43,000 (everything that moves is reported every tick).

Server, 100 sessions, 40 ticks per mode (milliseconds, whole `GameplayService::tick`):

| Case | tick p50 | p95 | fan-out over baseline |
|---|---|---|---|
| Loaded world, no sessions | 373 | 394 | - |
| Loaded world, 100 sessions, fog on | 487 | 535 | +114 |
| Loaded world, 100 sessions, fog off | 478 | 488 | +104 |
| Idle world, no sessions | 47.0 | 47.4 | - |
| Idle world, 100 sessions, fog on | 162.2 | 163.7 | +115 |
| Idle world, 100 sessions, fog off | 165.4 | 167.0 | +118 |

That is about 1.1 ms per session per tick of pure view building, even when nothing
changes. Wire traffic per session, loaded world: fog on 28.5 KB/s mean (45 KB/s max),
of which fog state is 27.7 KB/s (a full 96 x 96 `FogState` of 9.2 KB is resent
whenever the vision revision moves, which is nearly every tick) and unit deltas only
0.8 KB/s; fog off 7.4 KB/s. The first snapshot is 23.6 KB (it contains all 1,000
units of the primary player, whichever session asks). The mean changed units per
session per tick is only 0.15 because the views cover 96 x 96 tiles around a base.
Encoding and draining all 100 sessions took 0.1 to 0.24 ms per tick p50 (the
encoder is not the bottleneck).

Before and after the two quick wins (20 players, 20,236 units, 200 ticks,
`reports/scale/before-memo-20p.json` and `after-memo-20p.json`): `advance` p50
234.2 to 33.4 ms, movement 211.8 to 18.5 ms, world build 10.6 to 2.1 s, canonical
hashes identical at the initial state and at ticks 100 and 200
(`3d7274f2...`, `bd6954b7...`, `9b7ca262...`). At 100 players the first run before
the changes (an earlier workload without house foundations; its JSON was
overwritten) gave `advance` 866 p50 / 1,098 p95 / 1,344 max, movement 776, order
issue 111 p50; after: 381, 421, 797, 294 and 2.6.

## Profile (callgrind, `valgrind` in the analysis image)

Before the quick wins, whole-run inclusive instructions: 60% in `advance_movement`;
of that 51% in route planning (`poll_world_route_planner`) and 9% in `unit_step_clear`;
57% of everything in `evaluate_landscape_inner_with_base`, the procedural terrain
(blake3 hashing, shoreline noise, seeded rivers and lakes) evaluated per single tile
with no reuse. After the memo: at 40 players, 59% of the movement window is still the
terrain evaluation, now first-touch misses: each mover enters a new tile every
about 25 ticks, 41,000 movers give about 1,600 tile entries per tick, each costing
17 to 60 us of procedural evaluation. Cold costs measured by the benchmark
(`terrain_probe`): 16.9 us per tile as a point query, 4.4 us per tile when a whole
32 x 32 chunk is generated (3.8 times cheaper).

## Quick wins applied (no behaviour change, replay hashes identical)

1. **Per-generator tile memo.** `MapChunkGenerator` memoizes the composed tile and
   resource of a tile (`tile_at`, `object_at`, `tile_and_node`) and the base physical
   tile used by route edge checks, in two direct-mapped 131,072-slot tables behind a
   mutex (a fixed budget, asserted at most 40 MiB per generator, allocated on first
   use; the browser build compiles zero slots and keeps no memo) (`crates/map/src/terrain/point_memo.rs`, used from
   `terrain/landscape_dispatch.rs`). The generator is immutable and deterministic;
   only successful, uncancelled answers are stored; every builder that changes its
   inputs resets the memo. Tests assert equality with direct evaluation. Gain at 20
   players: movement 11.5 times cheaper; at 100 players about 2.3 times on the whole
   tick (the rest is first-touch evaluation, hot spot 1).
2. **Wildlife near-animal skip** (`wildlife/ai.rs:7`): a unit whose cell is not next
   to any occupied animal cell is skipped after one hash lookup; the counters stay
   identical. Gain is small (51 to 42 ms at 100 players, the cells at this density are
   almost always occupied); the real fix is hot spot 5.

Also added (no gameplay effect): `WorldLimits` (defaults equal the shipped caps),
`advance_with_probe`/`TickPhase`, `GameplayService::from_scale_world`.

## Top 10 hot spots

Complexity uses U units, M movers, P players, B buildings, S sessions, A animals,
V vision radius. Gains are estimates for 100 players, not measurements, except where
stated.

1. **Per-tile terrain evaluation, uncached or first-touch**:
   `crates/map/src/terrain/landscape_dispatch.rs:130` (`memoized_point`), called per
   unit per tick from `crates/simulation/src/game_movement.rs:79` through
   `crates/simulation/src/terrain.rs:106`. O(M) lookups per tick, O(new tiles) full
   evaluations. Fix: a chunk-level passability and resource bitmap cache (32 x 32
   chunks, generated in bulk, 3.8 times cheaper per tile, shared by movement,
   planner, gathering and placement) with prefetch of the chunks ahead of movers.
   Expected: movement terrain cost 4 to 10 times lower.
2. **Route planning edge checks**: `crates/map/src/navigation/planner/terrain.rs:45`
   (`route_edge_clear` to `edge_between_with_cancel`, `terrain/surface.rs:59`):
   two base-tile evaluations per expansion; was 49% of the profile before the base
   memo. Order issue was 111 ms p50 per tick before it. Fix: the same chunk cache plus
   hierarchical pathfinding (portal graph per chunk) so long routes (1,600-tile
   attack orders currently fail with `NoAttackPosition`, 1,073 of 1,073) cost
   O(chunks) not O(tiles). Expected 5 to 10 times on planning.
3. **Sequential per-mover step**: `crates/simulation/src/game_movement.rs:19` to
   `:200`, O(M) per tick, 294 ms, single-threaded, with a `BTreeMap` id lookup and
   large `StoredUnit` records (`game_world.rs:110`, `:93`). Fix: dense storage indexed
   by entity id, structure-of-arrays for position and order, a parallel (rayon) pure
   proposal phase over fixed unit shards, then a sequential apply in id order so the
   replay hash stays identical. Expected 6 to 8 times on 12 cores on top of 1 and 2.
4. **Vision**: `crates/simulation/src/game_world/visibility/mod.rs:286`
   (`remember_visible_buildings`: P x B x footprint `BTreeMap` lookups every tick,
   17.6 ms at 724 buildings, grows with P x B so about 100 times that at 5,000
   buildings) and `:76` (`apply`: a disc of `BTreeMap<TileCoord,u32>` and overview
   pixel updates per tile crossing, about 16 ms). Fix: recompute building memory only
   when a building or its visibility changed; per-player chunk bitsets and counters
   instead of `BTreeMap`; update only changed edges of the disc; run per player in
   parallel (players are independent). Expected 15 to 30 times.
5. **Wildlife scan**: `crates/simulation/src/game_world/wildlife/ai.rs:7`, O(U x 9)
   `BTreeMap` lookups every tick, a 42 to 47 ms floor even with every unit idle.
   Fix: iterate animals and query the unit chunk grid (`GameWorld::chunks`), and
   sleep animals with no unit within sight. Expected 42 to under 3 ms.
6. **Replay hash**: `crates/simulation/src/game_world/hash.rs:31`, 47 ms per call,
   O(U + route tiles + vision). Not per tick today, but any per-tick or per-second
   desync check at 100k units is unaffordable. Fix: per-chunk hashes combined in a
   Merkle tree, only dirty chunks re-hashed, blake3 on worker threads. Expected 10
   times, or checkpoint every N ticks.
7. **Gathering and economy**: `crates/simulation/src/game_world/economy/gathering.rs:284`
   (`advance_gathering`, 17.4 ms), `:86` (`nearest_resource`, O(r^2) generator calls
   per search, 5.4 s for the initial assignments), `economy/dropoff.rs:32`
   (`dropoff_order` scans every building of every player, allocates and sorts, per
   return trip). Fix: per-player drop-off index and a per-chunk resource node list
   from the chunk cache. Expected 3 to 5 times.
8. **Result collation**: `crates/simulation/src/game_world/economy/tick.rs:69`:
   sort, dedup, retain and re-fetch of 43,000 `GameUnit` copies every tick (8.9 ms),
   plus a `Vec` rotation of all movers in `game_movement.rs:32`. Fix: dirty list by
   dense index, delta emission per chunk.
9. **Server view building**: `crates/server/src/gameplay/economy.rs:14`
   (`query_units`: per session, a `BTreeSet` of candidates, an O(U) scan of all
   units of the viewer's player, then a visibility lookup per unit),
   `crates/server/src/gameplay/tick.rs` (a per-session `BTreeMap` of unit states, a
   clone of the player and building maps per session at `:224`). 1.1 ms per session
   per tick, O(S x (view + own units)). Fix: build one shared per-chunk change list
   per tick, let sessions subscribe to chunks (interest management), send each unit
   delta once per chunk, and run the fan-out off the tick thread on the previous
   tick's snapshot. Expected 10 times, and out of the tick budget.
10. **Fog on the wire**: `crates/server/src/gameplay/fog.rs:42` (`state`: 9,216
    `BTreeMap` lookups per session, a full tile array resent when the vision
    revision changes, 27.7 KB/s per session). Fix: per-player dirty rectangles in
    chunk bitsets, deltas (run-length encoded) only for changed chunks inside the
    viewport. Expected 20 to 50 times fewer bytes and CPU.

## Hard limits found in the code (independent of speed)

- `MAX_ECONOMY_PLAYERS = 16` (`simulation/.../economy.rs:34`, wire cap
  `protocol/src/gameplay/economy.rs:9`): with 100 players the fog-off snapshot
  failed to encode for all 100 sessions (`encode_failed`).
- `MAX_ECONOMY_BUILDINGS = 256` world-wide, `MAX_GATHERERS = 80`, `MAX_BUILDERS = 80`,
  `MAX_HUNTERS = 80` world-wide, `MAX_ANIMALS = 4,096`. Run with raised caps here.
- `MAX_POPULATION = 200` per player: 100 players cap out at 20,000 units by rules.
  The 100,000 units of this benchmark are placed directly and bypass training.
- `MAX_SUBSCRIBED_UNITS = 16,384` and 1 MiB messages per session.
- The service has one viewer: every session is filtered through the primary unit
  owner's fog and receives that player's units; there is no per-player subscription
  and no AI controller in the code base (the driver here is a stand-in).
- `initialize_village` gives the starting resources kit only to the first village.

## What reaching the target takes

Budget for a 50 ms tick on 12 cores, single machine, authoritative simulation plus
server (estimates, to be re-measured per step):

| Item | Now (ms) | Target (ms) | How |
|---|---|---|---|
| Movement and planning | 294 | 8 to 15 | chunk terrain cache (4x), hierarchical paths, dense storage, 6 to 8 way parallel |
| Vision and building memory | 34 | 2 to 4 | bitsets, dirty updates, per-player parallel |
| Wildlife | 42 | 2 | animal-driven query on the unit grid |
| Gathering and economy | 17 | 4 | drop-off and resource indexes |
| Collation | 9 | 1 | dirty list |
| Server fan-out | 115 | 8 to 12 | shared per-chunk deltas, off-thread, interest management |
| Replay hash | 47 | 5 (or off tick) | Merkle by chunk, N-tick checkpoints |

That sums to about 25 to 40 ms for the tick if the gains hold, leaving little margin.
Dedicated hardware qualification and finished RTS content (combat at scale, real
pathing congestion, real source terrain) are outside this measurement. Required
design decisions, in order: raise the caps and the population rule (or decide the
real unit target is 20,000), build the chunk terrain cache and hierarchical paths,
move to dense unit storage, introduce deterministic parallel execution (shard by
unit index, merge in id order, hash-verified against the sequential build), and
rework the server around per-player interest-managed deltas.

## Limits of this evidence

- Synthetic procedural terrain, not a real France package. Source-backed maps read
  pages and have different per-tile costs; the memo and chunk-cache gains may be
  smaller there. Rerun against a real package before relying on the absolute
  numbers.
- The driver is not an AI: orders are random, 35% of military moves are rejected as
  unreachable, long attack orders are rejected, and 30 bases had no wood in range,
  so only about 41,400 of 101,111 units move. A real economy keeps more villagers
  walking. Scale the movement line accordingly (about 7 us per mover per tick).
- One run per configuration; no variance across seeds. The committed
  `perf-scale.json` is a later rerun with other agents loading the machine (1-minute
  load about 11): `advance` 413 p50 / 556 p95 / 790 max, movement 318, server tick
  with 100 sessions 663 against a 492 baseline. The tables above are the quieter
  earlier run; the verdict does not change. Ticks are measured with
  other containers on the machine idle but not isolated.
- The 100-player "before" numbers come from an earlier run of the same command with
  an earlier workload, quoted from its console output. The kept before/after pair
  is the 20-player one with identical replay hashes.
- Server numbers are for the shipped single-viewer design; they say nothing about a
  true per-player design except the per-session view cost.

## Commands

- `make perf-scale` (default workload) or
  `PERF_SCALE_ARGS="--players 20 --ticks 200 --fog-off-ticks 0 --server-ticks 0 --out reports/scale/x.json" make perf-scale`;
  options: `--players --units-per-player --side-tiles --ticks --fog-off-ticks
  --orders-per-second --seed --fight-size --hash-every --server-ticks --subscribers
  --idle-world --out`.
- Idle server fan-out: `PERF_SCALE_ARGS="--idle-world --ticks 20 --fog-off-ticks 0 --server-ticks 40 --out reports/scale/perf-scale-idle-server.json" make perf-scale`.

## Movement (branch `perf/movement`, revision 4c07decb)

Same workload and command as above (`make perf-scale`, 100 players, 101,111 units,
600 ticks fog on, 100 fog off). Raw reports: `reports/scale/before-movement.json`
(revision 7867e9a3) and `reports/scale/after-movement.json` (4c07decb, clean tree);
`after-cache.json` is the intermediate run after the chunk cache alone.

| Milliseconds | before p50 | before p95 | after cache p50 | after all p50 | after all p95 |
|---|---|---|---|---|---|
| Movement and pathfinding | 300.6 | 380.6 | 37.4 | **26.1** | **40.4** |
| Whole `advance` | 390.5 | 490.5 | 143.1 | **103.5** | 165.4 |

Movement is 11.5 times cheaper at p50 and 9.4 times at p95 (target: 4 times). The whole
`advance` is 3.8 times cheaper; the rest (wildlife, vision, gathering, collation) is the
next target, see hot spots 4 to 8. Fog off: movement 272.5 to 21.5 ms. Memory is 105 MiB
resident (was 103). The machine was shared with other agents' builds, so absolute
numbers carry noise of some percent; the ratio is far outside it.

**Determinism.** All 12 canonical hashes (ticks 100 to 600 fog on and the 100 fog-off
ticks) are identical before and after. The 4-player scenario in
`crates/harness/src/perf_scale/tests.rs` pins three hashes (ticks 100, 200, 300)
computed at 7867e9a3; run it with
`HARNESS_TEST_FILTER=small_scale_scenario make test-harness-focused` (`make test-unit`
does not run the harness crate's unit tests).

**What changed.**

1. *Chunk terrain cache* (`crates/map/src/terrain/walk_cache.rs`). The first touch of any
   tile of a 32 x 32 chunk generates the whole chunk once (cost per tile 3.8 times lower
   than point queries) and keeps bitmaps: passable, walkable surface, mud, plus height
   levels and the resource nodes. Edge rules (`edge_between`), walkability, step cost
   and the simulation's `passable`/`crossable` read these instead of evaluating terrain.
   Bounded to 64 MiB with least-recently-used eviction (about 20,000 chunks). A page
   provider failure falls back to the exact per-tile path; cancellation is honoured before
   every hit. A test compares every cell and edge with direct evaluation, including a map
   edge that cuts a chunk. Caveat: a cached chunk no longer asks a page provider again, so
   `resumed_provider_failure_is_a_typed_stopped_reason` now fails on terrain not read before.
   Not done: active prefetch of the chunks ahead of movers. Generation is deterministic and
   total work is identical, so prefetch only moves cost between ticks; at 100 players every
   base's neighbourhood is warm after a few seconds.
2. *Hierarchical planner* (`crates/map/src/navigation/hierarchy.rs`). Portals are open
   entrance pairs on chunk borders (one per run); intra-chunk costs are computed lazily
   by a chunk-confined Dijkstra and cached; the abstract search is weighted A* (weight 2,
   so the route costs at most twice the optimum) bounded to 8,192 abstract expansions.
   It is built from static terrain and resource nodes only, never the dynamic overlay, so
   a result never depends on cache state. Tests: wall with and without a gateway, a
   1,600-tile route across about 50 chunks within 2,048 expansions, a cold-cache twin, the
   bound. The leg planner (`RoutePlanner`) is unchanged on purpose: it already commits
   legs of 48 tiles, so short routes stay bit-identical. It is not yet guided by portal
   waypoints (next step below).
3. *Far attack orders* (`combat/approach.rs`). The exact local search is unchanged. When it
   hits its 8,192-tile cap the order picks the nearest in-range open tile at the target and
   walks like any far move; within 400 tiles the portal graph first rejects a provably
   disconnected tile. Tests accept a 1,600-tile attack on a unit and on a Town Center.
   **The benchmark still reports 1,073 `NoAttackPosition`.** Measured with a debug print on
   a 2-player run: those attackers' local search ends after 4 to 22 tiles with the
   frontier exhausted, not at the cap, i.e. the synthetic militia are spawned in enclosed
   pockets of the procedural map. Distance is not their cause, so the gate is spawn
   placement in `world.rs`, not pathfinding.
4. *Per-mover step.* `EntityId -> slot` is a flat vector (`game_world/unit_index.rs`),
   and the rotation of the mover list by the planning cursor no longer copies it per tick.

**Next steps.** (a) Guide `RoutePlanner` legs with portal waypoints (leg target = next
portal, final target last) to avoid dead-end legs on lakes and rivers; keep the current
rule for routes shorter than two chunks. (b) Dense structure-of-arrays storage for
position, order and planner handles indexed by slot, so the mover loop touches two cache
lines per unit instead of a `StoredUnit`. (c) Rayon: a pure proposal phase over fixed unit
shards (next position, terrain verdict), then a sequential apply in id order, hash-verified
against the sequential build; `update_unit_vision` (a `BTreeMap` of observers) and
`building_step_clear` are the remaining serial parts. (d) Spawn military in open ground in
the benchmark so attack and move orders stop failing. (e) Chunk generation of 4.5 ms is
the cold cost; a base-terrain-only fast path would cut it where trees are not needed.
