# Movement rework: cap inventory and the semantic lint

Phase 0 of [the delivery plan](pathfinding-rework-delivery.md#2-phases); verifies the 17-line
inventory of [plan §4.10](pathfinding-rework.md#410-caps-removed-complete-inventory-phase-0-verifies)
against dev `19dc7fe47`. File:line is where the constant or the counter's refill lives. A *world
cap* limits per-tick behaviour for the whole match (units wait or are refused when it is spent); a
*capacity cap* limits how many things can exist. The removing phase follows delivery §2 with its
§5 amendments; where the delivery plan does not name a cap, the phase here is this inventory's
assignment and is marked *(assigned)*.

## 1. The 17 caps of plan §4.10, verified

| # | Cap | Today | Where | Kind | Removed by |
|---|---|---|---|---|---|
| 1 | Route search: fair work | 16,384 per tick | `simulation/src/lib.rs:26` (`MAX_ROUTE_WORK_PER_TICK`; refilled `route_scheduler/issue.rs:33`, spent `route_scheduler.rs`, `fast.rs`) | world, per tick | 4 |
| 1 | Route search: fast attempts | 65,536 per tick, 16,384 per player, 8,192 per attempt, 8,192 reserved for far groups | `game_movement.rs:30,33`, `:26` (`FAST_ROUTE_WORK`), `route_scheduler/fast.rs:8` | world, per tick | 4 |
| 1 | Route search: slots | 2 near + 2 far searches per player; 64 planners per player; 400 searches and 4,096 planners world-wide (2,048 shared) | `route_scheduler.rs:38,40,46`, `game_movement.rs:14,18`; checked `route_scheduler.rs:98-102`, `accounting.rs:117,154-157` | world, capacity | 4 |
| 1 | Route search: order work | 12.8M per order; 3 legs of commit work for a near route while others queue; 4 renewals of a far group | `lib.rs:22`, `map/src/navigation/planner.rs:30`, `route_scheduler/deadline.rs:18,29` | per order | 4 |
| 2 | Position searches | 4 per tick | `combat/approach.rs:32`, refilled `route_scheduler/issue.rs:35`, spent `approach.rs:146-157` | world, per tick | 4 |
| 3 | Attack searches | 16 per tick | `combat/approach.rs:20`, checked `approach.rs:72-77`, `combat/tick.rs:60` | world, per tick | 6 *(assigned: the melee perimeter-spot search, §4.8)* |
| 4 | Gather repicks | 4 per tick | `game_movement.rs:42`, spent `economy/gathering/replacement.rs:21-39` | world, per tick | 4 *(assigned)* |
| 5 | Navigation preparations | 8 per tick | `lib.rs:25`, refilled `route_scheduler/issue.rs:39`, spent `economy/navigation/preparation.rs` | world, per tick | 4 |
| 6 | Refinements | 4 per tick (2 reserved for formation members) | `route_scheduler/issue.rs:34`, spent `movement_step/refinement.rs:215-232`, `refinement/work.rs:8-11` | world, per tick | 4 (refinement deleted) |
| 7 | Detaches | 4 per tick (a literal) | `formation/detach.rs:188` (`.take(4)`) | world, per tick | 4 (`detach.rs` deleted) |
| 8 | Yields | 8 asks per tick, 2 bodies per ask | `movement_step/yielding.rs:13,14,192` | world, per tick | 2 (yields deleted); §5 lists the yield cap with phase 4 |
| 9 | Idle visits | 32 per tick, from 4 phase queues | `spacing/idle.rs:6,21-29` | world, per tick | 2 (idle push deleted, resting relaxation restored) |
| 10 | Idle acquisitions | 128 per tick | `combat/acquisition.rs:7,132` | world, per tick | 6 |
| 10 | Target inspections | 16,384 per tick, shared with towers (half for acquisition, 3/4 stop, 2,048 reserved for towers) | `combat/targeting.rs:9,11,91,151`, `acquisition.rs:32,135,197`, `towers.rs:116` | world, per tick | 6 |
| 11 | Formation looks | 16 engagements and 128 members per look; looks every 4 ticks; 256 candidates, 2,048 inspections, 128 per bucket, 1,024 building tiles | `formation/combat.rs:5-19`, used `:65-68,166-220` | world, per tick | 6 |
| 12 | Scouting | 8 jobs per tick, 16 active searches (and 64 refused tiles per job) | `economy/scouting/mod.rs:14,15,152-159,196` | world, per tick | 4 *(assigned: the searches become L2)* |
| 13 | Generals | 8 armies per 10 ticks (cadence 10, admissions 8) | `economy/general/mod.rs:23,24,309` | world, per tick | 6 |
| 14 | Server commands: per tick | 32 per tick match-wide | `server/src/gameplay/tick.rs:114,127` | world, per tick | 4 |
| 14 | Server commands: queues | 64 pending per connection, 1,024 global (scaled by seats), 256 per AI, 4,096 unacked | `protocol/src/gameplay.rs:42,45`, `server/src/gameplay/commands.rs:13`, `commands/queue.rs:25-49`, `deliver.rs:84` | capacity | 4 |
| 15 | `MAX_ECONOMY_PLAYERS` | 128; also sizes animals (`MAX_ANIMALS` 4,096 = 128 × 32) | `simulation/src/game_world/economy/limits/mod.rs:3`, `protocol/src/gameplay/economy.rs:17`, `wildlife.rs:19-21`, `protocol/src/gameplay/wildlife.rs:7` | capacity | 0 raises it at the simulation level; 7c lifts the wire cap |
| 16 | Depletion records | 2²⁰ (`1 << 20`); past it a depletion is refused (`Capacity`) | `map/src/overlay.rs:15`, checked `:124,266`, `overlay/snapshot.rs` | capacity | 4 |
| 17 | Obstacle tiles | 2²⁴ (`1 << 24`) | `map/src/overlay.rs:25`, checked `overlay.rs:84`, `overlay/bridges.rs:58` | capacity | 4 |

Rows 1, 10 and 14 span several constants. Every value matches the plan.

## 2. Caps found beyond §4.10

| Cap | Today | Where | Kind | Removed by |
|---|---|---|---|---|
| Waypoints advanced per mover loop | 4,096 per tick | `game_movement.rs:11,117` | world, per tick | 4 |
| Hierarchy proofs per attack search | 4 | `combat/approach.rs:17,374` | per search | 6 |
| Attack search tiles | 1,024 per search | `combat/approach.rs:12,214-246` | per search | 6 |
| Collision cell density | 32 bodies per fine cell | `movement_step/collision/index.rs:9` | capacity; kept as the over-cap rule | 2 (§4.11, over-cap cells admit only outward moves) |
| Carrier members | 4,096 per formation | `core/src/formation.rs:4` (`MAX_FORMATION_MEMBERS`) | capacity | 5 |
| Move destinations per order | 16 | `core/src/lib.rs:26` | capacity | 5 *(assigned)* |
| Navigation cache | 8,192 entries, 128 MiB | `navigation_cache.rs:8,11` | capacity | 4 (corridors replace it) |
| Resource cache | 8,192 entries, 32 MiB | `game_cache/resources.rs:6,7` | capacity | 4 *(assigned)* |
| Clearance pages | 4,096 | `terrain/clearance.rs:11` | capacity | 4 *(assigned: overlay pages)* |
| Army size | 200 members (squads of 10) | `economy/general/mod.rs:18`, `general/squads.rs:5` | capacity | 6 |
| Army survey | 1,024 visits | `economy/general/survey.rs:33,39` | world, per decision | 6 |
| Tower candidates | first 64 buildings | `combat/towers.rs:112` | world, per tick | 6 |
| Gather approach | 8 failures, 8 candidates | `economy/gathering.rs:11`, `gathering/approach.rs`, `gathering/replacement.rs` | per unit | 4 *(assigned)* |
| Detach retries | 8, every few ticks | `formation/detach.rs:14` | per unit | 4 |
| Build groups and queues | 32 | `protocol/src/gameplay/economy.rs:24`, `economy/construction/continuation.rs:5` | capacity | outside the rework |
| Buildings, jobs, animals per player | 8,192; 65,536; 32 | `economy/limits/mod.rs:5,8`, `wildlife.rs:19` | capacity | 7b (generational ids, event-driven economy) |
| Subscribed units, tiles per viewer | 65,536; 512 | `protocol/src/gameplay.rs:40,41` | capacity | 7c |
| Buildings on the wire | 16,384 | `protocol/src/gameplay/economy.rs:20` | capacity | 7c |
| Planner starting search | 12,000 samples, 384 sites, 512 candidates, 128 anchors | `economy/start_site.rs:8,9`, `start_search/robust.rs:8`, `economy/starting_kit.rs:10` | match creation only | not per tick; stays |

Economy and AI rules that look like caps but are game rules or AI budgets are in the allowlist
because the lint matches names, not intent, and they stay: farm storage, market price,
`VASSAL_LIMIT`, `MAX_WALL_SEGMENTS`, the Steward's per-domain order and search budgets
(`economy/steward/rules.rs`, `manager.rs`), `MAX_APPROACH_ATTEMPTS`, `MAX_BRIDGE`, `MAX_NODES`.
Geometry locals named `budget` (`contact.rs`, `formation/anchor/taut.rs`, `avoidance.rs`) are
name-based false positives of the lint, not caps.

## 3. Forbidden per-tick passes

Eight passes over every unit, building, animal, journey or session run each tick or each map
change. They are counted, not capped: cumulative `{calls, visited}` pairs that are diagnostics only
(never hashed, never snapshotted, never read by behaviour; `GameWorld::pass_counts`,
`simulation/src/game_world/route_scheduler/passes.rs`, server counter in
`server/src/gameplay/probe.rs`). Phase 1 takes each `visited` per tick to 0.

| Counter | Pass | `visited` is |
|---|---|---|
| `collision_opened` | `resources.rs:123`, every unit on each tree felled or building removed | units |
| `collision_closed` | `resources.rs:150`, every unit on each building placed or gate closed | units |
| `remember_visible_buildings` | the per-tick calls of `economy/tick.rs` (event-driven callers are not counted) | watched buildings cloned |
| `advance_gates` | `economy/construction/gates.rs:36`, every building | buildings |
| `wildlife_ids` | `wildlife/ai.rs:7`, every animal's id collected | animals |
| `advance_transports` | `navy/journeys.rs:134`, every journey | journeys |
| `collect_changed` | `economy/tick.rs`, every changed id sorted and fetched | ids |
| `tables_refresh` | `Tables::refresh` in `movement_step/mod.rs`; player values only, with job ownership read on demand for each proposed mover | 0 (the all-job scan was removed) |
| `unit_frame` | the server's `UnitFrame::build`, initial population or subsequent simulation change ids | unit ids (including removals) |

`unit_frame_converted` and `unit_frame_changed` (in the same `passes`, `visited` only) count the
units the frame converted afresh and the units whose wire state changed. The simulation journals
unit and report mutations at their source; retained frames update only those ids. Unchanged
states, indexes and owner-list entries stay in place, and sessions holding the previous frame
skip unchanged states in their diffs. A missed tick or unavailable previous frame rebuilds once.

They are exposed in `GET /admin/match/routes` under `passes`, and the scale-match report copies
that object into each minute's `routes.passes` (take the delta between minutes).

## 4. The semantic lint

`make architecture-check` (and so the pre-commit, Stop and CI static gates, `crates/harness/src/
architecture/caps.rs`) parses every production source in `crates/simulation/src`,
`crates/server/src/gameplay*` and `crates/map/src/overlay*` with `syn`, skipping test files,
`#[cfg(test)]` and `#[test]` items and the benchmark and demo workload builders. It records four
shapes whatever the names:

| Kind | Meaning | Example |
|---|---|---|
| `use` | a path to a constant named like a cap (`MAX_`, `PER_TICK`, `BUDGET`, `LIMIT`, `CAPACITY`, `SLOTS`, ...) | `MAX_COMMANDS_PER_TICK` |
| `cmp` | a counter-like operand (`used`, `left`, `budget`, `searches`, `pending`, ...) compared to a constant or a literal of at least 2, a length compared to a constant, or a drained budget compared to anything | `self.combat.position_searches == 0` |
| `take` | `.take` or `.truncate` of a constant or literal of at least 2 | `.take(4)` |
| `refill` | a counter-like place assigned a constant, or a budget decremented | `self.local_refinement_left = 4` |

`gates/movement-caps.json` holds today's findings as `{file, kind, expr, count}`; line numbers are
not part of the key, so unrelated edits do not touch it. It **ratchets**:

- a finding beyond its allowance is a new cap and fails (an AI-visible message names file, line and
  shape);
- an allowance beyond the findings is stale and fails too, so the PR that removes a cap must lower
  or delete the entry in the same change;
- an allowance beyond its count at the merge base with `refs/remotes/origin/dev` fails, even
  when a new cap and its matching allowance are committed together. Missing comparison history
  fails closed; fetch `origin/dev` before checking. Pre-commit uses the source repository's
  history while checking the exported index's sources and manifest;
- nothing is ever added. A new cap in the rework is not allowed; a rule that is not a cap must be
  written without a counter compared to a constant.

When the merge base predates this lint's manifest, its production sources establish the initial
allowances. This also covers the previously omitted `gameplay_match.rs` and `gameplay_match/`
sources: initial entries there must already exist at the base, never originate in the change.

To regenerate after a deliberate removal, delete the file and run `make architecture-check`: the
failure prints the current findings in the file's format. Review the diff; it may only shrink.
Whole-line shrinkage per phase is the phase's proof that it deleted its caps.
