# Roadmap: caps and budgets to remove

Owner 11-23: "We need to solve, not limit and hide." Each cap below hides a cost or a design shortcut. The owner's
decision per cap (11-23) is in the status cell. Survey of 157 caps on dev of 11-22
(`gates/movement-caps.json` plus per-tick budgets, rule structs, `.take(N)`, the Python AI config and the browser
far view). Statuses and rules: [roadmap](../roadmap.md). Dates are 2026 (MM-DD).

## How to remove them without losing the tick rate

1. Measure first, in an `exp/` branch, never on dev: one switch sets every cap below to unlimited; replay the
   Anatolia 4 war state (scale session's 750-tick recording) and record instructions per tick per phase and the worst
   tick. This shows which caps hold the tick up and which never bind. Not measured yet.
2. Caps that never bind or cost nothing: drop together in one PR.
3. World-wide counts become per-entity intervals: "8 domains a second for the whole world" makes each domain slower
   as the game grows; "each domain every N seconds" keeps behaviour fixed at any size and makes cost grow only with
   the number of domains. Then only the cost per entity is optimised.
4. Caps that hold the tick up ship together with their fix (cheaper work at the source), never alone.
5. The perf gate (instructions per tick on replays, CI perf comment) is the guard, not the cap. Work still deferred is
   visible as a wait metric, never dropped silently.

Every movement and route cap goes to the scale session ([scale](scale.md), #978; owner 11-23 "give it the movement
caps"): route budgets are removed once their cost is fixed, the other movement caps as decided below. Combat, economy,
orders, display and AI caps are in the landscape lane.

## Movement and routes

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Route work per tick, world-wide (16,384) | 11-23, 11-25 x2 | DONE: dropped (#1096) | `MAX_ROUTE_WORK_PER_TICK`, crates/simulation/src/lib.rs (#72). Units ordered across rivers and forests wait for a route; 11-19 bigger budgets cost +3.2% to +4.9% ([scale](scale.md)); 11-25 (owner: 'the route caps are still not removed so I can't do shit'): no world, player or slot number left; every waiting search is polled every tick on the workers with a 65,536-work slice of its own, never refused or queued (only per-search bounds stay) Merged in #1096. | Done; next: cut the cost per search (#978) |
| Fast-route budget (65,536 per tick, 16,384 per player, 8,192 per order) | 11-23, 11-25 x2 | DONE: dropped (#1096) | crates/simulation/src/game_movement.rs (#566): short trips must never wait behind long searches; short trips queue when it is spent; 11-25 (owner: 'the route caps are still not removed so I can't do shit'): no world, player or slot number left; every waiting search is polled every tick on the workers with a 65,536-work slice of its own, never refused or queued (only per-search bounds stay) Merged in #1096. | Done; next: cut the cost per search (#978) |
| Far group routes (8,192 per tick) | 11-23, 11-25 x2 | DONE: dropped (#1096) | crates/simulation/src/game_world/route_scheduler/fast.rs (#716): a slice of the route budget reserved for formations marching far, so one army does not starve villagers; 11-25 (owner: 'the route caps are still not removed so I can't do shit'): no world, player or slot number left; every waiting search is polled every tick on the workers with a 65,536-work slice of its own, never refused or queued (only per-search bounds stay) Merged in #1096. | Done; next: cut the cost per search (#978) |
| Route searches per player (2 near + 2 far) and route planners (64 per player, 2,048 shared) | 11-23 x1 | DONE: dropped (#1096) | route_scheduler.rs (#566, #716). Searches are new long searches in progress; planners are found routes extended leg by leg. Both only share the world budget fairly between players; 11-25 (owner: 'the route caps are still not removed so I can't do shit'): no world, player or slot number left; every waiting search is polled every tick on the workers with a 65,536-work slice of its own, never refused or queued (only per-search bounds stay) Merged in #1096. | Done; next: cut the cost per search (#978) |
| Local detours (4 per tick) | 11-23 x1 | PLANNED (scale): fix, then drop | `local_refinement_left`, crates/simulation/src/game_map.rs: a small search around blocked neighbours; in a jam hundreds need one | Make the detour search cheap, then drop |
| New resource after depletion (4 re-picks per tick) | 11-23 x1 | PLANNED (scale): drop | Gathering re-pick budget; low impact | Drop |
| Step-aside requests (8 per tick, 2 bodies each) | 11-23 x1 | PLANNED (scale): drop | crates/simulation/src/game_world/movement_step/yielding.rs (#710): a blocked unit asks friends in its way to move; jams clear slowly | Drop the per-tick count; keep each unit's 120-tick wait before asking |
| Lost formation member retries (8 tries, every 256 ticks) | 11-23 x1 | PLANNED (scale): drop | crates/simulation/src/game_world/formation/detach.rs (#710): after 8 tries the soldier stays where it gave up | Drop the try count, keep the 12.8 s interval (one route per stranded soldier); real fix: stop members getting stranded |
| Idle crowd spreading (32 per tick) | 11-23 x1 | PLANNED (scale): drop | crates/simulation/src/game_world/spacing/idle.rs (#567) | Drop |

## Combat

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Attack-move look limits: 16 soldiers sent, 128 members checked, 256 enemies considered per look | 11-23 x1 | DONE: dropped (#1026) | crates/simulation/src/game_world/formation/combat.rs (#617, #719). Every member that sees an enemy it can fight engages on the look; each chunk near the group is read once per look, only where another player is present. 200 archers in a box all engage on one look (0 on dev). R1 war replay (Anatolia 4, 750 ticks, dev 5383c79) 481.6M to 512.2M instr/tick mean (+6.3%), p95 647.2M to 668.3M (+3.3%), combat phase 16.6M to 71.4M, movement 261.5M to 238.2M; first hits 2,585 to 4,607, hits 21,748 to 30,215, kills 357 to 545 (with the idle acquisition change below) | Recheck in play with large ranged armies. |
| Attack-move look interval (every 4 ticks, 0.2 s) | 11-23 x1 | PLANNED: keep | Same file; staggered by group id | Keep |
| Idle soldiers looking for a target (128 per pass, world-wide) | 11-23 x1 | DONE: dropped (#1026) | #1026: `MAX_ACQUISITIONS` removed: every awake soldier looks on its own turn, one tick in four; asleep ones (no other player in their chunks) cost nothing. A look reads a per-chunk, per-viewer list of visible enemies built once per combat step and skips every 8-tile cell of its chunks with no visible enemy or enemy building tile. 600 idle archers all acquire within 4 ticks (127 on dev). R1 war replay (Anatolia 4, 750 ticks, dev 5383c79) 481.6M to 512.2M instr/tick mean (+6.3%), p95 647.2M to 668.3M (+3.3%), combat phase 16.6M to 71.4M, movement 261.5M to 238.2M; first hits 2,585 to 4,607, hits 21,748 to 30,215, kills 357 to 545. Still the main cost: awake soldiers near enemies repeat their look every 4 ticks | Skip a repeated look whose cells did not change; a sleep proof per 8-tile cell needs cell crossings from movement |
| Target-finding work (16,384 inspections per tick) | 11-23 x1 | DONE: dropped (#1026) | #1026: Removed with the 128-entry bucket window that let a dense own crowd hide an enemy (a look now reads every visible enemy of its chunks) | Merge |
| Firing-spot searches (16 per tick) and spread-out position searches (4 per tick) | 11-23 x1, 11-24 (moved to the scale session) | DONE: dropped (#1029) | crates/simulation/src/game_world/combat/approach.rs (#597, #606). Merged in #1029: R1 491.4M to 483.2M instructions a tick, kills 364 to 367, first blows +7.8%; attackers no longer re-ask every tick after a refusal. A far melee attacker that finds every spot taken now walks up instead of waiting | Consider a per-target cache only if searches grow with army size. |
| Arrows drawn (4,096) | 11-23 x1 | DONE: dropped (#1026) | #1026: `MAX_PROJECTILES` removed from protocol decode and validation, server view and client effect map; the 16 MiB gameplay frame bounds a message. 5,000-shot round trip, view and client effect tests | None (merged with the browser suites green) |
| Melee attackers per enemy (8) | 11-23 | PLANNED: keep | Game rule: room around a target | Keep |

## Economy, Steward, Vassal, General

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Steward domains per second (8, world-wide) and work per visit (64 orders, 8 resource searches, 32 site checks) | 11-23 x1 | DONE: dropped (#1038) | Each domain is visited once a second on its own phase; the per-visit order, search and site counts are gone. A domain's villagers are a member list kept by movement's tile changes and placement on the map; the unit census and building sweep run only after activation or a zone change. Idle villagers with no job retry on a doubling interval (1 to 32 s), are relocated at most once until they work, and the watchdog no longer stops workers waiting for a route. Fixed issue 1021. R1 replay (453c26853 vs clean dev 4d37e220a): 839M instr per tick (dev 480M); Stewards manage 13,819 moving villagers a tick (dev 64) and 22,333 accepted orders in 37 s (dev 260); orders replaced within 10 s 338 (dev 23). Owner 11-24: the cost was agreed Merged in #1038; recheck in play. | Profile the Steward pass (+220M issuer_steward) with call graphs; cut its cost per job |
| Vassal domains per second (8, world-wide) | 11-23 x1 | DONE: dropped (#1038) | Each Vassal domain is visited once a second on its own phase, whatever the number of Vassals; visits read the domain's member list (own units and visitors, kept by movement's tile changes) instead of querying the 201-tile box, which is queried only after activation or a zone change. R1 replay issuer_vassal 34.0M instr per tick (dev 15.4M) Merged in #1038; recheck in play. | A longer interval (owner: may act much less than once a second) |
| Wall tiles per placement (10) | 11-23 x1 | DONE: dropped (#1038) | Dropped: a wall run extends as far as the line is straight and the 24 paid sites in flight allow. Merged in #1038; recheck in play. |  |
| Steward villagers (100), Steward and Vassal radius (100 tiles), Vassal defenders (20) | 11-23 x1 | PLANNED: keep (design) | rules.rs, vassal/mod.rs (#571, #688) | Keep |
| Not decided 11-23: Vassal placements per pass (8), sites in flight (24), towered work points (8), roads 64 nodes and 14-tile bridges, builder next foundation (10 tiles, 4 tries), build order villagers (32), wall segments per order (64) | 11-23 | PLANNED | My read 11-23: keep sites in flight; drop the rest | Decide with the measurement of step 1 |
| General admissions (8 per 0.5 s) | 11-23 | DONE: dropped | General registration and removal maintain phase buckets. Every General gets one decision every 10 ticks, staggered by seat. The 1,000-army regression checks coverage and per-decision work. | Keep the per-General cadence |
| General army size (200 soldiers) | 11-23 | KEEP: design | The army composition and squad system use 200 as the designed maximum. | Keep |

## Orders, UI and AI

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Order rate (10 per second, burst 64) and order queue (64 per player, 1,024 server-wide, 32 run per tick) | 11-20, 11-23 x2 | DONE: #1020, #1028 | Anti-flood (#6, #551, #679); "queue full" when placing many buildings (11-20); #1020: 256 orders/s, burst 1,024, 2,048 pending per connection, 16,384 server-wide, every ready order runs each tick; #1028: gather, patrol, garrison, hunt and General walk go as one list order per 4,096 units | Raise a lot; the real cost is the routes the orders start, covered by the route rows |
| Display caps: far-view buildings 2,048, armies 256, towns 64, resource markers 2,048, Kingdom activity 6 rows, selection 4,096, far-view placement burst 20 | 11-22, 11-23 x2 | DONE: #1020, #1028 | Browser overlay rows and message sizes (#929, #957, #969, #805, #570); no simulation cost. Far view: [UI](ui.md); #1020 removed the 2,048-site, 256-army, 64-town, 2,048-marker, 6-row and far-placement-burst limits; selections go in messages of up to 4,096 units | Send in chunks, draw everything |
| AI per-decision caps: orders 40, route queries 16, site checks 4, idle villagers 16, train orders 4 or 8 | 11-23 | PLANNED | ai/aoe_ai/config.py, play/economy.py (#551, #567, #803). My read 11-23: drop all; keep the 48-tile march hops | Decide with step 1; AI strength is not the focus (owner 11-20) |
| Safety limits kept: training queue 15 and batch 5, 16 waypoints, 99 AIs, 65,536 units sent to a screen, message sizes, ship cargo 5, view refresh 150 ms | 11-23 | PLANNED: keep | Memory and rule bounds | Keep |
