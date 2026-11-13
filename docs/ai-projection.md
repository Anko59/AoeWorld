# AI expeditions

The ordinary public-SDK Brain keeps one expedition at a time. From Castle Age,
after twenty game minutes, sixteen unled soldiers and one free builder gather
at a reachable staging point. Their reservations keep home rallies, Generals
and economy reassignment from taking them away. A mission has a thirty-minute
timeout and a one-minute retry delay.

The expedition follows explored land toward a distant known enemy building,
or explores toward the map centre when none is known, then cycles eight
map waypoints if the centre contains no target. Soft production buildings
are preferred to Town Centers and castles. A blocking river can receive a
bridge in Castle or Imperial Age as well as Feudal Age. A shoreline instead
starts a transport trip: one ship carries at most four soldiers and a builder,
waits for observed boarding, follows known water, then unloads on an explored
shore toward the goal, rejecting a proven land connection back home. Native water
scouting can escape a stalled directional water hop. Only passengers appearing after the voyage
prove a landing.

Near a target at least eighty tiles beyond a completed Town Center, the builder
raises a Barracks and Siege Workshop. Local production grows the landing force
and supplies rams before an attack. Optional science waits while the expedition
is active; age advancement and the first Castle army upgrade retain priority.
The behaviour is implemented in `ai/aoe_ai/play/projection/`.

## Public SDK queries

`World.water_passable(tile)` returns known navigable water, known dry ground,
or unknown. `World.water_frontier(from, toward)` searches at most 8,192 nodes
through explored water and does not cut diagonal shore corners. Python exposes
the coordinate arguments separately, like the existing land queries.
`Client.focus_view(tile)` requests a bounded 128-tile window around the mission;
`None` restores automatic following. It does not reveal unexplored terrain.

The manager receives four orders from the existing forty-order decision budget,
and advances at most once per four game seconds. Reinforcements join from spawn events, without a per-decision army scan.
The crew is capped at sixteen;
the ferry probes a fixed seventeen-by-seventeen shoreline neighbourhood. Land
and water searches share the existing sixteen-query decision budget. View
changes also consume the manager's order allowance and are throttled.
These bounds are design limits, not a scale-ladder performance qualification.

## Observation and qualification

The admin match summary adds lifetime enemy buildings destroyed, unit losses
(including passengers lost with a transport), and the latest building kill's
attacker, target, tile and tick. These counters update on resolved death only,
are excluded from gameplay hashes, and are reset with a fresh world. They are
never inputs to a Brain.

The benchmark observer records those counters per seat together with completed
bridges, distinct loaded transports and completed forward buildings. It samples
the latest building kill each game minute and keeps at most sixty-four records
per seat; intervening kills may be missed. A qualifying record must match the
mission's target and crew, follow an observed crossing, and lie at least eighty
tiles from its origin and every own completed Town Center seen by the observer.
The observer keeps the first territory sample for that kill; it is sampled after
the kill, not an exact territory snapshot at the kill tick. Bridge evidence records a unit seen on the deck and then
at the far bank; ferry evidence records boarding, ship travel and landed units.
Counts alone do not prove an attack crossed water. Local viewer pages for each
slot are removed when the benchmark exits and must never be committed.

The full economy control is recorded in [AI scaling](ai-scaling.md). Expedition
qualification requires a complete current 120-minute run and a matching crossing
and building-kill record. Until that evidence exists, force projection remains
unqualified. Capture a video only when it shows a real in-game attack.

## Current measured limit

The attempt on 2026-11-13 stopped at the brief's 3h15 cutoff, before any
30/60/120 checkpoint. Command:

```sh
make ai-benchmark AI_BENCHMARK_ARGS='--map-hash fdc59853e2404cb5c7fb245f21768970aeb01febe20fd4f15dc96ff522fc9a49 --starts qa/ai-scaling.json --wall-minutes 4 --out .cache/ai-scaling/ai3-projection.json'
```

Parent revision: `3b534409b24489beb128205142fa712895505608`, plus the
candidate's 1,050-entry source manifest. The report retains `complete=false`;
Make exited 2 after intentional SIGINT at 16:53:23 UTC. The last observer sample
was tick 32,400 (27 game minutes), with 199.97 wall seconds. Its timing stream
contains 32,400 ticks, p50/p95/max 1,313/4,348/105,590 microseconds and 3,254
applied orders. Concurrent compilation shared the host. These early ticks do
not establish late-game cost, live RTS performance or a complete result.

| slot / case | enemy buildings destroyed | units lost | bridges completed | transports used | forward buildings completed | qualified attacks |
| --- | --- | --- | --- | --- | --- | --- |
| 0 / island | 0 | 0 | 0 | 1 | 0 | 0 |
| 1 / forest | 0 | 0 | 0 | 0 | 0 | 0 |
| 2 / forest | 0 | 3 | 0 | 0 | 0 | 0 |
| 3 / land | 0 | 0 | 0 | 0 | 0 | 0 |

The public trace reports one expedition per seat and one island ferry landing.
There were no known enemy buildings in the last public trace. A landing count
alone does not prove a different landmass or a successful attack.

The final source adds tested recovery for a lost loaded ship, expired forward
foundations and waiting for an unfinished bridge; it also corrects far-bank
proof, existing forward production, home-coast unloading and territory checks.
Those changes, plus spawn-driven reinforcement and replacement training by a
surviving forward builder, continued exploration and native-enum-safe observer
serialization, postdate the prefix and have no benchmark qualification.
One Rust navy test was edited during the prefix; no runtime production source
changed while it ran. No measurement is presented as a final-source 120-minute
result. No qualifying attack video exists, so the conditional video is skipped.

## Control and missing final checkpoints

The current control is economy commit `3b534409b24489beb128205142fa712895505608`,
with the complete `ai3-growth.json` samples below. The expedition attempt has no
30/60/120 sample for any seat, and its later fixes were not benchmarked. Every
`unmeasured` cell applies to all final-candidate economic and combat fields.

| minute | slot / case | control pop / TCs / cells | age / techs | food / wood / gold / stone | military / kills | castles / towers / walls / gates | Stewards / Vassals / Generals | final expedition candidate |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 30 | 0 / island | 80 / 2 / 3 | CASTLE / 4 | 21 / 203 / 235 / 449 | 32 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 | unmeasured |
| 30 | 1 / forest | 78 / 1 / 2 | CASTLE / 5 | 69 / 150 / 181 / 627 | 34 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 | unmeasured |
| 30 | 2 / forest | 85 / 2 / 2 | CASTLE / 11 | 3 / 170 / 66 / 395 | 42 / 0 | 0 / 1 / 0 / 0 | 0 / 0 / 0 | unmeasured |
| 30 | 3 / land | 56 / 1 / 2 | CASTLE / 7 | 508 / 5 / 355 / 370 | 18 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 | unmeasured |
| 60 | 0 / island | 203 / 4 / 4 | IMPERIAL / 11 | 326 / 1004 / 1438 / 177 | 95 / 0 | 2 / 0 / 0 / 0 | 1 / 0 / 1 | unmeasured |
| 60 | 1 / forest | 183 / 2 / 3 | IMPERIAL / 6 | 71 / 842 / 191 / 1293 | 124 / 1 | 1 / 0 / 0 / 0 | 1 / 0 / 0 | unmeasured |
| 60 | 2 / forest | 242 / 4 / 4 | IMPERIAL / 21 | 1288 / 842 / 377 / 685 | 136 / 0 | 2 / 1 / 0 / 0 | 1 / 0 / 1 | unmeasured |
| 60 | 3 / land | 196 / 3 / 4 | IMPERIAL / 8 | 31 / 60 / 57 / 359 | 112 / 0 | 2 / 0 / 0 / 0 | 0 / 0 / 2 | unmeasured |
| 120 | 0 / island | 945 / 7 / 4 | IMPERIAL / 53 | 94960 / 0 / 1453 / 103 | 444 / 0 | 3 / 5 / 14 / 1 | 6 / 3 / 3 | unmeasured |
| 120 | 1 / forest | 1395 / 22 / 13 | IMPERIAL / 53 | 39354 / 102696 / 94 / 51 | 657 / 2 | 3 / 20 / 222 / 5 | 7 / 3 / 2 | unmeasured |
| 120 | 2 / forest | 1667 / 16 / 11 | IMPERIAL / 53 | 82690 / 12313 / 777 / 87 | 988 / 0 | 5 / 15 / 299 / 7 | 8 / 4 / 5 | unmeasured |
| 120 | 3 / land | 876 / 8 / 6 | IMPERIAL / 53 | 75741 / 2 / 55 / 38 | 322 / 0 | 2 / 4 / 0 / 0 | 7 / 2 / 2 | unmeasured |
