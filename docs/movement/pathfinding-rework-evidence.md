# Pathfinding rework: evidence

Companion to [the rework plan](pathfinding-rework.md), version 4. Sections 1–5 were read at `4e6ca00c5`; reviews read `19dc7fe47` (#735 adds only a probe). It records:
- the current system at dev `4e6ca00c5` (2026-11-07);
- the owner's reported pain points and their status on that revision;
- the measurements;
- prior art;
- the adversarial review log (§6).

Code citations are relative to `crates/`. Statuses come from PR text, fixtures and playtest notes;
"OPEN" means no fix has landed or the fix is unconfirmed in a real match.

## 1. Current system

### 1.1 Long-range planner (`map/src/navigation/**`)

| Aspect | Today |
|---|---|
| Grid | 2 m tiles, 32×32-tile chunks; maps of 64 to 262,144 tiles per side (`map/src/request.rs:10`) |
| Passability | Two domains: land (generator terrain) and naval (`NavalNavigation`, flat tile A\*, no hierarchy). No unit-size class; the planner treats units as points |
| Edge rule | Both cells passable and walkable, height difference ≤ 1 (`hierarchy/grid.rs:108`). Costs 1024 straight, 1448 diagonal, mud ×1.5; roads only change speed |
| Hierarchy | Single-level HPA\*: one portal per open border run (at its midpoint), plus directed diagonal portals. Dense P×P u32 intra-chunk costs. Per-chunk Kosaraju SCCs; no global component ids |
| Abstract search | Weighted A\*, w = 2 (`hierarchy.rs:42`), so routes may be up to 2× optimal. 8,192 expansions; endpoint attach 9,472 work |
| Tile search | 8-neighbour A\*, octile heuristic, `BTreeSet`/`BTreeMap` open set (`planner/search.rs:7-12`). Resumable per probe; legs commit at 48 tiles of progress or 65,536 work. Per-order cap 12.8M work |
| Guidance and pockets | Plain A\* first, then portal guidance after 16,384 work or 64 tiles; 1,024-tile reverse BFS pocket proof; certified-nearest flood (`prepared/resolution.rs`) |
| Dynamic obstacles | `ResourceOverlay` stamps (XOR of blake3) per chunk; the 3×3 neighbourhood key selects the static graph or a 64 MiB LRU `PortalCache` rebuild. The graph is never repaired in place |
| Gates | Always open to the planner (`economy/navigation.rs:130-138`); closed only by collision |
| Static table | Baked off tick by 4–8 workers, memory-mapped from a sealed memfd. Caps: 2 GiB file and 1M-chunk index, about 32k tiles per side |
| Determinism | Integer, BTree ordering; full planner state hashed per unit (`game_world/hash.rs:168`) |

### 1.2 Route scheduler (`simulation/src/game_world/route_scheduler/**`)

- **Budgets.** All are per tick and world-fixed (`lib.rs:26`, `game_movement.rs:14-33`):

  | Budget | Amount |
  |---|---|
  | Fair work | 16,384 |
  | Fast work | 65,536; at most 8,192 per attempt and 16,384 per player |
  | Reserved far-group work | 8,192 |
  | Chunk preparations | 8 |

- **Slots.**
  - Per player: 2 near and 2 far searches (`route_scheduler.rs:36`), and 64 planners.
  - World-wide: 4,096 planners and 400 searches.
  - "Far" means more than 64 tiles (Chebyshev).
- **A unit waiting for a route does not move.** There is no fallback motion.
  - Near waits expire at 1,200 ticks, far waits at 3,600 ticks, and far groups get 4 renewals
    (`deadline.rs:18-29`).
  - The module comment records a 100-player match that froze for hours behind near searches into
    sealed ground.
- **Sharing.** Only formations share a route: one carrier plans for up to 4,096 members. Single
  units share only identical-endpoint `NavigationCache` hits (8,192 entries).
- **Threading.** Planning is single-threaded on the tick thread.
- **Map changes.** `collision_opened` and `collision_closed` loop over every unit on each tree felled
  or building placed (`simulation/src/game_world/resources.rs:123-165`).

### 1.3 Local movement (`simulation/src/game_world/movement_step/**`)

- **Positions.** `i32` at 1,024 subunits per tile. Speed is derived per tick. Radii come from the
  DAT (205 infantry, 256 cavalry, 461 siege, 512 ship body with 256 terrain radius;
  `contact.rs:19-46`).
- **Per-unit state.** `StoredUnit` (`simulation/src/game_world.rs:122`) holds:
  - `route: VecDeque<WorldPosition>` of tile centres;
  - `planner: Option<RoutePlanner>`;
  - an obstruction memo, scheduler fields and the unreachable memo.
- **Pipeline** (`game_movement.rs:38-358`):
  1. rejoin detached members;
  2. a fair search pass;
  3. parallel proposals (rayon, shards of 128);
  4. a serial apply that re-sweeps bodies (`movement_step/mod.rs:385-395`);
  5. formation prepare, propose and apply;
  6. yields, then a vision flush;
  7. attack-move looks;
  8. lookahead planning and queued moves.

  The tick counter then increments mid-pipeline (`:356`).
- **Collision is a hard swept-circle veto with special cases.** There is no push or swap. The
  mechanisms that overlap on "a unit is in the way":
  - graded hard shares;
  - the 32-tick soft pass (`settling.rs:22`);
  - single-file pass-through;
  - the oncoming-lane exception;
  - the villager work squeeze;
  - the General-alone exception;
  - the narrow-water half radius;
  - an 8-direction sidestep with side memory;
  - a 17×17 quarter-tile refinement Dijkstra, 4 per tick world-wide;
  - lateral escape;
  - yields after 120 ticks;
  - an idle push every 4 ticks;
  - detach and rejoin;
  - a 240-tick give-up;
  - arrival replacement.
- **Spatial structures updated per step:**
  - the collision `HashMap` (1-tile cells);
  - the `chunks` BTreeMap (32 tiles);
  - wildlife cells (8 tiles);
  - `idle_spacing`;
  - `destinations.by_tile`.
- **Clearance** is lazy 64×64 pages in a `BTreeMap` behind a global `Mutex`, locked on every lookup
  (`simulation/src/terrain/clearance.rs:14`). The planner does not use it.
- **Bridge decks.** About 55 `deck` references sit in `movement_step` and `formation`.

### 1.4 Formations (`simulation/src/game_world/formation/**`)

- **Group.** Keyed by its leader unit, which carries the order, route and planner, with a virtual
  anchor and a polyline path. Shapes are Line, Box, Staggered and Flank. Slots are assigned once,
  in class bands.
- **Anchor.** It moves at the slowest member's pace, braked up to 1/8 by laggards. Members may catch
  up at 1.5×.
- **Narrowing.** Clearance is sampled at 5 points around the anchor
  (`formation/steering.rs:108-126`). It drives `packing.rs` file counts, and the block blends back
  after the pass.
- **Stragglers.** After 120 blocked ticks a member detaches with its own scheduler route, at most 4
  per tick. It rejoins within half a tile, on order end, or after being wedged 240 ticks.
- **Direct orders** (under 20 tiles, or a spread wider than the trip) steer straight at slots.
  Since #731 they also need clear lines.
- **Attack-move.** It looks every 4 ticks (256 enemies, 128 members, 16 peel-offs). After a kill a
  member returns as a new one-unit attack-move group (`resume.rs`).

### 1.5 Generals (`simulation/src/game_world/economy/general/**`)

- **Army.** At most 200 soldiers, a 50-tile influence and up to 10 typed squads (infantry 20,
  archers 15, cavalry 10, siege 5). Each squad is an ordinary Line formation order to an
  arm-specific offset (`squads.rs:92`).
- **Decisions.** Every 10 ticks, for at most 8 armies (round-robin), from a census of up to 1,024
  entries. At 10,000 Generals one army would re-decide about every 10 minutes *(est.)*.
- **Fights.** Squads of 20 attack-move at class-prioritised targets, and a 16-unit micro window
  handles wounded and siege units.
- **Player orders override the army** (#711).

### 1.6 Size and churn

| Measure | 2026-10-27 | 2026-11-07 |
|---|---|---|
| Production lines (movement, formation, route scheduler, spacing, navigation, General) | 9,544 | 17,176 |
| In-module test lines | 3,493 | 4,269 |
| Movement test directories (`game_world/tests/{formations,movement,routes,…}`) | n.m. | 12,213 |
| Commits touching these modules since 10-15 | | 47 |

The commit bodies of the last three movement PRs show the pattern of local fixes:
- **#727:** a pass probe that straddled the narrowing (files 20 → 2 → 20).
- **#731:** a ram stuck at a diagonal deck corner; direct orders pressing onto river banks; a dead
  member refusing the whole order.
- **#732:** a memo answering for the wrong overlay.

## 2. Owner pain points (status at `4e6ca00c5`)

Sources: playtest and brief notes from 10-23 to 11-07, PR bodies #604 to #732, `docs/movement/*`.
GitHub has no movement tracker: #265 is the closed landscape list.

| Theme | Pain point (first → latest report) | Status | Evidence |
|---|---|---|---|
| Responsiveness | Route queue always full; orders wait minutes or expire silently (10-23 → 11-07) | **OPEN.** #705, #716 and #732 partly fixed it; `routes/human-first` is unlanded | 11-05: 1,268 waiting, oldest 3,572 ticks (the owner's Paladin), 49,678 orders expired. 11-06: 655–688 waiting, 40–44 players at the search cap. The owner agrees this is the root of most drops (11-07) |
| Responsiveness | "Starts immediately" (a strength to protect) | Holds only when a route is cached or fast | Waiting units do not move (§1.2) |
| Coherence | Lines form too early, do not re-form after a pass, shapes compete (10-30 → 11-06) | **PARTLY**, #727 | Type order is lost by per-tick lateral re-ranking; lines snap ~12 tiles early; staircase passes re-rank at each step (#727 notes) |
| Coherence | Default line too sparse compared with AoE2 (10-30) | UNKNOWN | — |
| Obstacles | Mixed groups go straight at the destination and stick at the first obstacle (10-29 → 11-06) | **PARTLY**, #668 and #731 | 11-06 probe: speed 0.43 of nominal, stall 45%; no measured gain over the previous build |
| Obstacles | Partial obstacles (a tree patch, a bridge): some go around, the rest never path through (11-06) | **PARTLY**, #710 and #731 | Fixtures only |
| Obstacles | Unreachable targets leave armies pressed at the water (11-03) | **PARTLY**, #682 | 11-06 probe: 93 group orders ended unreachable or invalid, 35 wait expiries; `movement/no-wait-drops` unlanded |
| Recovery | Routes drop: units idle, or jitter on a couple of tiles (10-23 → 11-06) | **PARTLY**, #716 and #731 | 11-05: 2,515 of 5,238 moving soldiers spent 2+ minutes under 1 tile/min. Near groups still end at 1,200 ticks (`deadline.rs:62`) |
| Recovery | Soldiers blocked by their own route (11-05 probe) | **OPEN** | 44 detours each in 31 min; stall 35–77%; 0.22 of nominal speed; 1,713 planned tiles for 200 straight. Recovery rounds 2 and 3 doubled route work and were not shipped |
| Recovery | 400 rams with Generals at a pass, forest and river | **OPEN** | 299, 297 and 340 of 400 arrive |
| Chokepoints | Front crosses, the rest stay on the shore (10-30) | Fixed in fixtures (#634, #639, #668) | The owner rejected #649 ("they don't manage to pass the river at all"); no real-match confirmation |
| Generals | Followers walk in straight lines and do not recover; "mounted unusable" (10-30 → 11-05) | **PARTLY**, #668 and #689 | Typed squads exist; no coordination between squads at chokepoints |
| Attack-move | ~60% of units that should engage do (11-06) | **PARTLY**, #719 | 55–75% within 6 tiles; no after number |
| Ships | Ship route cost | Fixed, #732 (3.45×) | Ships were 70% of fast route work on 11-06 |
| Ships | Transports and shallows untested in play (10-26) | UNKNOWN | — |
| Villagers | Single units cannot pass each other: left-right-left forever (10-29 → 11-06) | **PARTLY**, #729 (rightward tie) | "Original shuffling problem was not reproduced" (review); the 48-tick side flip stays |
| Oscillation | Reversals on long marches | **OPEN** | The 1,000-tile General march pins 26 reversals; its stall pin went from 32 to 57 ticks in #710 |
| Tick rate | Late game at ~10 ticks/s (target 20) | **OPEN** | §3 |

The owner's view of good movement: "Movement was in a great state" (10-30 build). Small groups that
"recovered, found the bridge, lost nobody" (11-03, paraphrased in the note).

The owner's tests are hard real terrain, the real-match probe instead of "96%", and in-game
before/after footage.

## 3. Measurements

| When | Workload | Result |
|---|---|---|
| 10-22 | Synthetic `perf-scale`, 100 players, 101,111 units (41.5k movers), 6 CPUs | `advance` 45.9 / 66.7 ms (p50 / p95, fog on); movement and planning 29.2 ms p50 |
| 10-22 | France, 99 AIs, seeded to 90k units | 120 ms p50 tick, 0.35× real time |
| 10-28 | Owner match, ~11k units | 53 ms tick p50; movement ~40 ms, planning ~13 ms |
| 11-04 | Owner 5.4 h match, ~11k units | 20 Hz for the first hour, then 11.5–16 ticks/s; p50 62 ms, p95 104 ms. A 6-hour host limit silently killed all AIs |
| 11-05 | Quiet machine, 99 AIs, 10.4k seeded units | 19.65 ticks/s, 43 / 66 ms |
| 11-06 | Owner late game, France, 100 players | **9.5–10.1 ticks/s**, 97 ms p50; planning 56 ms (`plan_fast` 48), movement ~53 ms |
| CI | Late-game workload, 14,000 units | ~200–211M instructions per tick, about 15k per unit per tick |
| Synthetic | `perf-groups`, 10,900 units in 200 armies, 12 workers | Movement 4.8 / 5.8 ms; ~4k instructions per unit per tick; ordered apply is the largest stage |

Movement plus pathfinding is 58–75% of the tick in every view. #727, #731 and #732 have not been
re-measured on a late-game match. No measurement exists beyond 100 players or ~101k units.

**Phase 1 events (`rework/phase1-events`, 11-09).** Gates, building memory and the map-change unit
scans moved to indexes and events (`0d2db5af5`, `9201245df`); naval clearance pages keep their
generator inputs (`26de3a16a`). On the generated 14,000-unit late-game workload, 400 ticks, an
uncommitted probe run through `make test-native-focused` (dev profile, not release):
- canonical hashes equal to the base `8045708ab` at every 50th tick;
- whole-population passes removed per 400 ticks: `collision_closed` 400 calls / 2,781,000 units
  visited, `remember_visible_buildings` and `advance_gates` 400 / 160,000 each; all now 0;
- ticks 101–400, p50 base → events: building memory 0.353 → 0.260 ms, construction
  0.013 → 0.003 ms; the whole tick (85.2 → 82.4 ms p50) is within run-to-run noise.

Not measured: release `perf-late-game` and `scale-match --late-game` (the long-job slot was held by
other agents), and whether naval clearance rebuilds are what produced the owner's `terrain_point`
spikes (p50 2.5 ms, p95 7–13 ms, max 17–48 ms per tick in the 11-06 match, 120 fishing ships and
104 galleys at minute 32).

## 4. Prior art

| System | Design | Demonstrated scale | Lesson for us |
|---|---|---|---|
| AoE2 (Pottinger 1999, [part 1](https://www.gamedeveloper.com/programming/coordinated-unit-movement), [part 2](https://www.gamedeveloper.com/programming/implementing-coordinated-movement); [postmortem](https://gamedeveloper.com/design/postmortem-ensemble-studio-s-age-of-empires-ii-age-of-kings)) | Commander paths, members chase slot offsets; slowest speed; scale, break-and-re-form or halve at chokepoints; retained collision resolutions; three pathfinders (long-range mip-mapped, tile, polygonal) | 8 players, ~200 units each | The behaviour contract; side memory against oscillation |
| AoE2 DE patch notes (81058, 141935, 153015, 177723, 185872) | No assembly before moving; straighter formations; fewer breaks around small obstacles | — | B1, B3, B4; DE still lists big groups at chokepoints as known (`group-movement-rewrite.md` §1) |
| **AoE4** ([GDC 2022, Cheng](https://media.gdcvault.com/GDC+2022/Speaker+Slides/Pathing+In+Age_Cheng_Frank+2022-03-29+00.16.38.pdf)) | Portal graph (HPA\*-style); flow fields generated only along the A\* route, segmented as units advance; a virtual formation leader on the flow path, with units steering to slots and falling back to the flow without line of sight; "extended flow" keeps mixed-size groups together | 1,600 units, 1024² grid; flow field 1.11 ms and steering 0.56 ms for 200 cavalry (~2.8 µs per unit) | **Closest published design to the plan**; we need ~10× cheaper steering and LOD to scale past it |
| SupCom2 flow-field tiles ([Emerson, Game AI Pro ch. 23](https://www.gameaipro.com/GameAIPro/GameAIPro_Chapter23_Crowd_Pathfinding_and_Steering_Using_Flow_Field_Tiles.pdf)) | Portal graph with merging A\*; per-sector fields cached by portal; one shared all-ones cost field for clear sectors (50–70% of space); island id per sector for "cannot reach" | Hundreds to thousands of agents | Fields keyed by exit portal; share clear-chunk fields; island ids |
| 0 A.D. ([hierarchical pathfinder](https://raw.githubusercontent.com/0ad/0ad/master/source/simulation2/helpers/HierarchicalPathfinder.cpp), [formations](https://raw.githubusercontent.com/0ad/0ad/master/binaries/data/mods/public/simulation/components/Formation.js)) | Per-chunk regions plus global regions per passability class; long-range JPS on navcells; vertex short-range pathfinder | ~1,000 movers on a 256² map (design target) | Global ids work, but a region split can force a full-map flood: hence the plan's two-level union-find. The short-range pathfinder was its bottleneck (unverified ticket snippet) |
| Kamphuis and Overmars 2004 ([paper](https://ics-websites.science.uu.nl/docs/vakken/mcrws/papers_new/Kamphuis_Overmars%20-%202004%20-%20Finding%20paths%20for%20coherent%20groups%20using%20clearance.pdf)) | A backbone path widened by its clearance into a corridor; members move freely inside it; coherence by bounded width | Groups up to 100, a few percent CPU | The frame's clearance profile (plan §4.5) |
| Van den Akker et al. 2010 ([MIG](https://webspace.science.uu.nl/~gerae101/pdf/MIG2010.pdf)) | One group is a dynamic-flow problem; several groups with different goals is NP-hard; greedy rules deadlock at chokepoints | — | A heuristic portal admission schedule (plan §4.7), not per-unit greed |
| HPA\* / DHPA\* / SHPA\* ([AIIDE](https://ojs.aaai.org/index.php/AIIDE/article/view/12397)) | Cluster abstraction; rebuild only damaged clusters; read-only search helps threads | ≤ 320² maps | Multi-level is standard; our maps need L2 |
| Contraction hierarchies, hub labels ([Abraham et al.](https://www.microsoft.com/en-us/research/wp-content/uploads/2012/04/MSR-TR-2012-46.pdf)) | Static road graphs; CH ~159 µs per query on 18M nodes | Western Europe | Too static for building-heavy maps; ALT on L2 is the update-friendly middle |
| CPD, JPS+ ([JAIR 2015](https://jair.org/index.php/jair/article/download/10971/26118), [Game AI Pro 2](https://www.gameaipro.com/GameAIPro2/GameAIPro2_Chapter14_JPS_Plus_An_Extreme_A_Star_Speed_Optimization_for_Static_Uniform_Cost_Grids.pdf)) | Precomputed first moves; jump tables | 754K nodes (CPD: 576 MB, ~1 h on 48 cores) | Do not scale to 10⁸–10⁹ cells with dynamic walls |
| ORCA ([paper](https://gamma.cs.unc.edu/ORCA/publications/ORCA.pdf)) | Per-agent linear program | 5,000 agents in 8–15.6 ms on 8 cores (~13 µs per agent-core) | Too costly per unit; integer separation with priority is enough for AoE-style "collide and correct" |
| Continuum Crowds ([Treuille 2006](https://grail.cs.washington.edu/projects/crowd-flows/continuum-crowds.pdf)) | Per-group potential field merging navigation and avoidance | 10k people at a few fps | Cost by cells, not agents; only for homogeneous groups |
| AoE1 network ([1500 archers](https://www.gamedeveloper.com/programming/1500-archers-on-a-28-8-network-programming-in-age-of-empires-and-beyond)); SupCom ([Smith](https://www.gamedeveloper.com/design/opinion-synchronous-rts-engines-and-a-tale-of-desyncs)) | Lock-step; SupCom ~8,000 entities, cannot join in progress | 8 players | At 10M units, server-authoritative with interest-managed deltas |
| Planetary Annihilation ([ChronoCam](https://www.forrestthewoods.com/blog/tech_of_planetary_annihilation_chrono_cam/)) | Server-authoritative 10 Hz; clients receive keyframed curves for visible units | 40 players, 1 Mbit per player | Send frames and curves, not per-unit positions |
| EVE Online ([time dilation](https://eveonline.com/article/introducing-time-dilation-tidi)) | The game clock slows on an overloaded node instead of dropping work | 6,557 concurrent in one battle | Precedent for P4: slow the clock, never silently drop orders |
| Factorio ([FFF-415](https://direct.factorio.com/blog/post/fff-415)); Quantum ECS | A desync came from results depending on core count | — | Fixed partitions, read-only inputs, ordered merges; results independent of thread count |

No published system shows per-unit pathing at 10M units or on maps of tens of thousands of tiles per
side. AoE4 (1,600 units), SupCom (~8,000 entities) and EVE (6,557 players) are the largest verified
figures. Beyond them, the design relies on hierarchy, shared per-goal data and level of detail.

## 5. Evidence gaps

- **Re-measurement.** #727, #731 and #732 are unmeasured on a late-game match. The 11-06 live
  profile is uncommitted (`AoeWorld-play9/.cache`). `perf-scale` was last run on 10-22.
- **Probe metrics.** The probe does not yet record:
  - slot error and class-order violations;
  - oscillation counts;
  - first-step latency for every order, rather than per group.
- **Unpublished internals.** Exact AoE2/DE formation shapes, radii for ships, and corridor
  head-on rules are unpublished. AoE4's multithreading talk is members-only.

## 6. Review log

### Round 1 (2026-11-07)

- **Format:** five adversarial reviewers, Opus 5.5 through pi on API credits, each with a persona
  and the whole-movement scope.
- **Cost (pi):** aoe2-fidelity $4.02, cost-auditor $6.47, delivery $3.18, failure-modes $4.86,
  memory-feasibility $5.10, total $23.63.
- **Inputs:** reviewers read dev `19dc7fe47`. Their reports are in `review/round1/`.

**Corrections to version 1:**
- The attack-search cap is 16 per tick (4 is the position-search cap).
- Gates are owner-only and evaluated every tick.
- The replay tests use 1, 2 and 6 threads on one uniform fixture.
- `canonical_hash` is not a live-tick cost.
- The 56 ms of planning came from the owner's older build: #735 measures 7.6–9 ms on dev.
- The machine is 6 cores / 12 threads, not 12 cores.

**All five verdicts on scale agree:**
- 1M units at 20 Hz is reachable only with exact LOD, sleeping systems, no O(units) passes, a
  sub-µs kernel and in-process AI.
- 10M at 20 Hz with every unit simulated individually is not reachable on this machine.

**What version 2 changed, and why:**

| Finding (reviewers) | Change in version 2 |
|---|---|
| Bookkeeping dominates the tick: vision (~420 core-ms at 3M), fan-out (~1.5 s at 10M), sorting changed lists, `active_movers` memmoves, all-unit map-change scans (cost, delivery) | Plan §4.9; phase 1; P1 forbids O(units) passes |
| Multi-rate and convoy LOD are not exact (all five) | Analytic movers and slot-locked convoys only; LOD oracle gate; multi-rate deleted (scale §4) |
| Version 1 sized 10M for a cluster; memory lines did not fit 14 GB (memory, cost) | Single-machine rungs and byte budget (scale §1, §3) |
| Baked clearance (4 GiB) and L1 (28 GB) do not fit; trees as 16 B records (memory) | Lazy clearance plus summary; compressed class-tagged L1 ≤ 700 B per chunk; trees as bits |
| Admission was a hidden queue; about ten other world caps make units wait (fidelity, failure, delivery) | Admit on the tick, coalesce, reject visibly; caps removed; lint |
| A 2-tile gap read as 1 tile; no +1 file; one gap for all classes; one file count per group (fidelity) | Cross-section width, per-class gaps, per-rank capacity (plan §4.5) |
| Curved wide ranks undefined at corners (fidelity) | Minimum turning radius; wide blocks wheel |
| Fields ignore unit blockers, so a unit wall pins a group forever (fidelity) | Blocker escalation (plan §4.3) |
| Soft separation stacks; enemies passable; side rule rigid (fidelity, failure) | Half-gap rule, hard enemies, pair side memory, crossing rule (plan §4.6) |
| Cliffs, dynamic decks, gate churn, `u8` labels, splits mid-march (failure) | One overlay grid with cliffs and gates as edges; dynamic decks; `u16`; sub-frames per (component, class) |
| P2 failed for spread groups (failure) | Goal-rooted corridors that settle every member's region |
| Field keys, L2 costs and corridors were not pure (failure) | Full keys; L2 and corridors as hashed state; cache oracles |
| Phase 2 needed a waypoint adapter; phases 4 and 5 were coupled; the premise was stale; there was no kernel proof (delivery) | Spike S1 with kill rule; bookkeeping phase; cutover merges corridors and fields; avoidance before frames; ~69–95 days |
| Deleted mechanisms without replacement: reservations, idle relaxation, return-to-work, hull rules, ejection, give-up (failure) | Kept or restored (plan §4.6); progress-based arrival |

### Round 2 (2026-11-07)

- **Format:** the same five reviewers, on version 2. Reports are in `review/round2/`.
- **Cost (pi):** aoe2-fidelity $2.80, cost-auditor $4.31, delivery $3.23, failure-modes $3.98,
  memory-feasibility $2.66, total $16.98.
- **Grades:**

  | Reviewer | Grade |
  |---|---|
  | aoe2-fidelity | 6 |
  | cost-auditor | 6 |
  | failure-modes | 6 |
  | delivery | 6.5 |
  | memory-feasibility | 6.5 |

- **Consensus:** phase 0 could start; S1 and the behaviour phases needed changes.

**What version 3 changed:**

| Finding | Change in version 3 |
|---|---|
| Phase 1 radix grid cannot keep replay identity, because today's apply is Gauss-Seidel and the salt depends on ids and slot order (cost, delivery) | Phase 1 keeps an incremental index and monotonic ids; the radix grid and Jacobi move to avoidance with one repin |
| An SoA permutation per tick would use the whole machine's bandwidth (cost) | Incremental 8 B (cell, slot) index; permute at epochs only |
| Wakes on 3×3-chunk presence fire every ~6 ticks at density, and convoys rarely form (cost, fidelity) | Time-to-contact wakes on 4-tile cells; armies form one convoy; calm share measured at R1 density in S1 |
| The vision design contradicted its budget (cost) | One design: `u8` counts per active chunk with edge updates |
| Missing passes: building memory clones every watched building per tick, per-session fog rasters, O(view) diffs under the lock, the gate scan, wildlife, transports, serial combat and gathering (cost, failure) | Event-driven in phase 1; pursuit parallel in phase 6; economy and combat sleep as phase 7b |
| Missing budget lines: AI, Generals, churn, frames (10–30 µs), wakes (all) | Scale §2 rewritten; R1 is a conditional target with a sensitivity table |
| No field-level unit layout; side memory "per pair"; no all-unit index (memory) | Hot record ≤ 64 B laid out; 2 fixed side slots; Morton presence index |
| Depletion cap 2²⁰ silently idles gatherers; obstacle cap 2²⁴; remembered buildings at ~250 B (memory) | Bitmaps, caps deleted, 24 B records, capacity lint |
| Three memory envelopes (memory) | 5 GiB shared for R1/R2, 8 GiB quiet for R3, 1.5 GiB for S1 |
| Per-rank refill moves slots backwards; rigid wheeling undefined (fidelity) | Taper ahead of narrowings, monotone slots, per-rank `F` frozen; rigid only with clearance |
| `h` undefined; sleepers could not be pushed; ship lane exactly at its limit (fidelity) | Hardness defined with all rows; wake on contact; hull `h` 0.45 |
| Arbitration scheduled enemies; escalation misfired in fights and raced arbitration (fidelity, failure) | Alliance-only arbitration with whole-frame leases; escalation for hard non-combat blockers only, with N > phase + lease |
| Gates contradictory, and the "slip in while open" play lost (failure, fidelity) | Gated portals for reachability; physical state in the hard constraint |
| Sub-frames split every mixed army (failure) | Split only when reachable sets differ; largest bucket; merge |
| Re-search on every opening flips routes (failure, fidelity) | Openings never re-search; closures only when they cut a portal run; 10% switch margin |
| Raster decks erode a ram's 51 su margin (failure, fidelity) | Analytic deck capsules |
| A recorded open-loop replay cannot gate behaviour (failure, delivery) | Closed-loop in-process match built in phase 0 |
| Snapshot oracle infeasible (failure) | Dropped |
| Incomplete cap inventory, with a name-based lint (failure, delivery) | Full inventory in plan §4.10; semantic lint |
| Phase 3 fields on the hard-veto step; adapter left in formations; estimates did not grow (delivery) | Avoidance before the cutover; detach and resume replaced in the cutover; about 116 days |
| S1 circular and lax: calm workload, 2× kill, wall gate on 8 threads (all) | Calm sweep, real terrain, frames, wakes, vision, player events; kill at 1.25×; instruction gate; 6 physical cores |

### Round 3 (2026-11-07)

- **Format:** the same five reviewers, final check on version 3. Reports are in `review/round3/`.
- **Cost (pi):** total $10.54. Total for the three rounds: $51.15.
- **Grades:**

  | Reviewer | Grade |
  |---|---|
  | aoe2-fidelity | 7 |
  | failure-modes | 7 |
  | cost-auditor | 7.5 |
  | delivery | 7.5 |
  | memory-feasibility | 7.5 |

- **Readiness:** all five allow phase 0 now. S1 needed specification edits: gate arithmetic, exact
  vision counts, presence buckets, pushing, census terrain, and the phase-0 counter reality.

**All of these are in version 4** (plan §4.11, delivery §5):
- pushee motion and outward moves from over-cap cells;
- joint side choice;
- formation-type masks;
- leases with aprons;
- closure re-search on a region split;
- deck edges in regions;
- permission-keyed gate fields;
- exact vision overflow;
- live building reads;
- bucketed presence;
- a sparse cell index;
- two hashes;
- T+1 wakes;
- per-portal escalation;
- the B5 slot-chain exemption and the B6 enemy case;
- S1 gates of 135 and 190 core-ms with a measured calm share;
- phase 1b hot/cold split;
- Callgrind-based byte gates;
- re-estimates.

No round 4 was run: the remaining items are specification edits the reviewers named exactly.
