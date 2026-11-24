# Roadmap: caps and budgets to remove

Owner 11-23: "We need to solve, not limit and hide." Each cap below hides a cost or a design shortcut. The owner's
decision per cap (11-23) is in the status cell; nothing here is started. Survey of 157 caps on dev of 11-22
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
| Route work per tick, world-wide (16,384) | 11-23 x1 | PLANNED (scale) | `MAX_ROUTE_WORK_PER_TICK`, crates/simulation/src/lib.rs (#72). Units ordered across rivers and forests wait for a route; 11-19 bigger budgets cost +3.2% to +4.9% ([scale](scale.md)) | Remove once the cost per search is fixed (#978) |
| Fast-route budget (65,536 per tick, 16,384 per player, 8,192 per order) | 11-23 x1 | PLANNED (scale) | crates/simulation/src/game_movement.rs (#566): short trips must never wait behind long searches; short trips queue when it is spent | Remove once the cost is fixed (#978) |
| Far group routes (8,192 per tick) | 11-23 x1 | PLANNED (scale) | crates/simulation/src/game_world/route_scheduler/fast.rs (#716): a slice of the route budget reserved for formations marching far, so one army does not starve villagers | Goes with the route budget (#978) |
| Route searches per player (2 near + 2 far) and route planners (64 per player, 2,048 shared) | 11-23 x1 | PLANNED (scale): drop | route_scheduler.rs (#566, #716). Searches are new long searches in progress; planners are found routes extended leg by leg. Both only share the world budget fairly between players | Replace both with round-robin between players (no number); dropping them alone lets one player take the whole budget |
| Local detours (4 per tick) | 11-23 x1 | PLANNED (scale): fix, then drop | `local_refinement_left`, crates/simulation/src/game_map.rs: a small search around blocked neighbours; in a jam hundreds need one | Make the detour search cheap, then drop |
| New resource after depletion (4 re-picks per tick) | 11-23 x1 | PLANNED (scale): drop | Gathering re-pick budget; low impact | Drop |
| Step-aside requests (8 per tick, 2 bodies each) | 11-23 x1 | PLANNED (scale): drop | crates/simulation/src/game_world/movement_step/yielding.rs (#710): a blocked unit asks friends in its way to move; jams clear slowly | Drop the per-tick count; keep each unit's 120-tick wait before asking |
| Lost formation member retries (8 tries, every 256 ticks) | 11-23 x1 | PLANNED (scale): drop | crates/simulation/src/game_world/formation/detach.rs (#710): after 8 tries the soldier stays where it gave up | Drop the try count, keep the 12.8 s interval (one route per stranded soldier); real fix: stop members getting stranded |
| Idle crowd spreading (32 per tick) | 11-23 x1 | PLANNED (scale): drop | crates/simulation/src/game_world/spacing/idle.rs (#567) | Drop |

## Combat

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Attack-move look limits: 16 soldiers sent, 128 members checked, 256 enemies considered per look | 11-23 x1 | PLANNED: drop | crates/simulation/src/game_world/formation/combat.rs (#617, #719). A look is one scan by an attack-moving group for enemies near its members; big armies engage in waves of 16 and march through fire (owner 11-22: AIs "walk straight at me") | Drop; measure the look's cost on the war replay |
| Attack-move look interval (every 4 ticks, 0.2 s) | 11-23 x1 | PLANNED: keep | Same file; staggered by group id | Keep |
| Idle soldiers looking for a target (128 per pass, world-wide) | 11-23 x1 | PLANNED: drop | `MAX_ACQUISITIONS`, crates/simulation/src/game_world/combat/acquisition.rs (#597): with 30k idle soldiers each looks only about every 20 s; likely behind idle groups not fighting enemies passing by | Drop with a cheap pre-check (any enemy in this area, from a per-area per-player presence map); cost not measured |
| Target-finding work (16,384 inspections per tick) | 11-23 x1 | PLANNED: drop | crates/simulation/src/game_world/combat/targeting.rs (#597) | Drop with the acquisition fix |
| Firing-spot searches (16 per tick) and spread-out position searches (4 per tick) | 11-23 x1 | PLANNED: fix, then drop | crates/simulation/src/game_world/combat/approach.rs (#597, #606): each search covers up to 1,024 tiles, per archer; big ranged armies start firing late | Compute firing spots once per target, not per archer; then drop |
| Arrows drawn (4,096) | 11-23 x1 | PLANNED: drop | Projectile draw bound (#532); damage still applies | Drop |
| Melee attackers per enemy (8) | 11-23 | PLANNED: keep | Game rule: room around a target | Keep |

## Economy, Steward, Vassal, General

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Steward domains per second (8, world-wide) and work per visit (64 orders, 8 resource searches, 32 site checks) | 11-23 x1 | PLANNED: fix, then drop | crates/simulation/src/game_world/economy/steward/rules.rs (#571): with more than 8 towns each economy runs less than once a second; 11-20 AI economy diagnosis blames the same pass | Each domain on its own interval; work per visit follows events (villager idle, resource gone) instead of a full census; Vassal issuing went 21.5M to 3.8M instr per tick this way (11-21) |
| Vassal domains per second (8, world-wide) | 11-23 x1 | PLANNED: drop | crates/simulation/src/game_world/economy/vassal/mod.rs (#721). Owner: Vassals "can act less, even much less than once a second, but it shouldn't change when I get more" | Per-domain interval, constant whatever the number of Vassals |
| Wall tiles per placement (10) | 11-23 x1 | PLANNED: drop | crates/simulation/src/game_world/economy/vassal/development.rs | Drop |
| Steward villagers (100), Steward and Vassal radius (100 tiles), Vassal defenders (20) | 11-23 x1 | PLANNED: keep (design) | rules.rs, vassal/mod.rs (#571, #688) | Keep |
| Not decided 11-23: Vassal placements per pass (8), sites in flight (24), towered work points (8), roads 64 nodes and 14-tile bridges, General 200 soldiers and 8 admissions per 0.5 s, builder next foundation (10 tiles, 4 tries), build order villagers (32), wall segments per order (64) | 11-23 | PLANNED | My read 11-23: keep sites in flight; drop the rest (General size is design) | Decide with the measurement of step 1 |

## Orders, UI and AI

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Order rate (10 per second, burst 64) and order queue (64 per player, 1,024 server-wide, 32 run per tick) | 11-20, 11-23 x2 | DONE: #1020, #1028 | Anti-flood (#6, #551, #679); "queue full" when placing many buildings (11-20); #1020: 256 orders/s, burst 1,024, 2,048 pending per connection, 16,384 server-wide, every ready order runs each tick; follow-up: gather, patrol, garrison and General walk go as one list order per 4,096 units | Raise a lot; the real cost is the routes the orders start, covered by the route rows |
| Display caps: far-view buildings 2,048, armies 256, towns 64, resource markers 2,048, Kingdom activity 6 rows, selection 4,096, far-view placement burst 20 | 11-22, 11-23 x2 | DONE: #1020, #1028 | Browser overlay rows and message sizes (#929, #957, #969, #805, #570); no simulation cost. Far view: [UI](ui.md); #1020 removed the 2,048-site, 256-army, 64-town, 2,048-marker, 6-row and far-placement-burst limits; selections go in messages of up to 4,096 units | Send in chunks, draw everything |
| AI per-decision caps: orders 40, route queries 16, site checks 4, idle villagers 16, train orders 4 or 8 | 11-23 | PLANNED | ai/aoe_ai/config.py, play/economy.py (#551, #567, #803). My read 11-23: drop all; keep the 48-tile march hops | Decide with step 1; AI strength is not the focus (owner 11-20) |
| Safety limits kept: training queue 15 and batch 5, 16 waypoints, 99 AIs, 65,536 units sent to a screen, message sizes, ship cargo 5, view refresh 150 ms | 11-23 | PLANNED: keep | Memory and rule bounds | Keep |
