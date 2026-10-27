# World capacities at 100 players

Every fixed bound in the simulation, map, server and protocol that a match of
100 seats at population cap 1000 (about 100,000 units, France 16,000 x 16,000
tiles) can reach in play, whether it is per world or per seat, and what happens
when it is reached. Sizes are set in `WorldLimits::for_match(seats, population)`
(`crates/simulation/src/game_world/economy.rs`); the match creation passes the
slot count and the population cap.

## The playtest failure

The owner's 99-AI France match, cap 200: shortly after Feudal Age every build
order answered "The order queue is full. Please try again shortly." Two
world-wide limits were reached, both refused as `EconomyError::Capacity`, and
the server mapped every `Capacity` to `RejectedQueueFull`:

1. the map overlay's building collision tiles, 4,096 for the whole world
   (100 Town Centers take 1,600; the rest is about 6 houses per seat), then
2. the world's 4,096 buildings (a Feudal village with houses, farms, camps,
   military buildings and walls is 80 to 130 buildings).

`a_hundred_feudal_villages_fit_and_building_orders_still_succeed`
(`crates/simulation/src/game_world/tests/scale_hooks/capacity.rs`) settles 100
Feudal villages (13,500 buildings, 31,000 collision tiles) and places a house
for every seat; `a_full_world_reports_a_world_limit_and_an_empty_group_a_too_large_order`
(`crates/server/src/gameplay/commands/tests.rs`) pins the honest results.

## Results the player sees

| Cause | Before | Now |
|---|---|---|
| A world capacity (buildings, collision tiles, gatherers, builders, hunters) | `RejectedQueueFull` | `RejectedWorldLimit`: "The world has reached its limit for that order." |
| An empty, repeated or over-32 villager group; a wall over 64 segments | `RejectedQueueFull` | `RejectedOrderTooLarge` |
| The session's or the server's pending order queue is full | `RejectedQueueFull` | unchanged (the only queue-full) |

The Python SDK mirrors both results (`CommandResult.REJECTED_WORLD_LIMIT`,
`REJECTED_ORDER_TOO_LARGE`). The example AI keeps a site refused for a world
limit (it is not a bad site) and stops placing buildings for
`world_limit_wait_seconds` (30 s).

## Simulation and map

"Reached?" describes storage headroom for 100 seats, cap 1000; it is not a runtime qualification.

| Capacity | Before | Now | Scope | When reached | Reached? |
|---|---|---|---|---|---|
| Buildings (each wall segment is one) | 4,096 | 1,024 x cap/200 per seat (at most 8,192): 512,000 at cap 1000 | world, sized per seat | `Capacity` on build, wall, Town Center | no: a walled Feudal village is ~135 |
| Building collision tiles (`MAX_OBSTACLE_TILES`) | 4,096 | 2^24; buildings x 16 bound it | world | `Capacity` on build; a gate stays open | no |
| Gatherers | 8,192 | seats x population: 100,000 | world | `Capacity` on gather/farm | no: one per unit |
| Builders | 2,048 | seats x population | world | `Capacity` | no |
| Hunters | 2,048 | seats x population | world | `Capacity` | no |
| Animals | 4,096 | 32 per seat: 3,200 | world | `Capacity`; start kit skips | no: 15 per village, no respawn |
| Projectiles in flight | 4,096 literal | seats x population | world | shot not launched (silent) | no: a shot lands before the next |
| Resource overlay changes (every depleted node, kept forever) | 65,536 | 2^20 (~24 MiB) | world | gatherer earns nothing and retries (silent) | after ~50 h (65,536 lasted ~3.5 h at ~5 trees/s) |
| Players | 128 | max(128, seats) | world | `Capacity` | no |
| Entity ids | u32, never reused | same | world | `EntityIdExhausted` | no (~10^6 in hours) |
| Corpses | none (30 s, then removed) | same | - | - | - |
| Farms | none beyond buildings; spent farms removed | same | - | - | - |
| Wall segments per order (`MAX_WALL_SEGMENTS`) | 64 | 64 | order | now `RejectedOrderTooLarge` | per order |
| Builders per order (`MAX_BUILD_GROUP`) | 32 | 32 | order | now `RejectedOrderTooLarge` | per order |
| Training queue | 5 | 5 | building | `RejectedTrainingQueueFull` | per building |
| Route work per tick (`MAX_ROUTE_WORK_PER_TICK`) | 16,384 | same | world, shared fairly per seat | orders wait (latency) | yes, by design |
| Active route planners | 4,096 (64 per seat) | same | world | unit waits for a slot (latency) | can bind under heavy orders |
| Active route searches | 256 (2 per seat) | same | world | waits; a stuck search ends `PathBudgetExceeded` | fits (200) |
| Navigation cache | 128 MiB / 16,384 chunks | same | world | evicts / clears | thrashes on France; latency only |
| Remembered buildings and resources per player | world bounds | building world bound; 2^20 resource entries | player | bounded by supported world data | grows with exploration |

## Server, protocol and SDK

| Capacity | Before | Now | Scope | When reached |
|---|---|---|---|---|
| Buildings in one message (`MAX_ECONOMY_BUILDINGS`) | 4,096 | 16,384 | message: own, seen and remembered buildings | remaining buildings arrive in bounded later deliveries |
| Gameplay frame (`gameplay::MAX_MESSAGE`) | 1 MiB | 4 MiB (largest combined tick ~1.8 MiB) | message | encode fails |
| Units in one message (`MAX_SUBSCRIBED_UNITS`) | 16,384 | same | message (subscribed region) | Error 413 instead of a tick |
| Projectiles in one message | 4,096 | same; the server sends the first 4,096 | message | trimmed (shown, never simulated, on the client) |
| Animals in one message | 4,096 | same | message | cannot pass the world's 3,200 at 100 seats |
| Resource changes in one message | 65,536 | same | message: under fog the viewer's known changes | larger snapshots use explicit offset/total pages |
| Session order queue | 64 human, 256 AI | same | session | `RejectedQueueFull` (a real queue) |
| Server order queue | max(1,024, sessions x 64) | same | server | `RejectedQueueFull` (a real queue) |
| Pending acks | 4,096 | same | session | session disconnected |
| SDK buffered events / dying memory | 4,096 / 8,192 | same | client | oldest dropped / set cleared |
| Stored-map resource snapshot | 4 MiB JSON | overlay entry bound x 64 + 1,024 bytes | stored maps only; matches never persist | oversized files rejected before replacement |

Building clients retain up to `128 x 8192` entries, covering the supported
world limit independently of the 16,384-entry message limit. The initial
snapshot and subsequent deliveries converge after camera changes/reconnects.
Resource clients retain up to 2^20 entries; each snapshot page stays within
the existing 65,536-entry wire bound. The browser assembles a complete snapshot
atomically, and the server reserves all pages before queue publication. These
are capacity/correctness bounds, not long-session performance qualification.

## Population 1000

The shipped default is 1,000 per seat. Houses still support five and Castles
twenty, capped by the match ceiling. Gatherers, builders, hunters and projectiles
scale to 100,000 at 100 seats; buildings retain the 200-population provisioning
ratio (5,120 per seat), rather than accidentally dividing by the new default.
The 2^24 collision-tile bound covers 128 seats at the 8,192-building maximum.
The 5,000 optional match ceiling and existing bounded wire deliveries remain.
Own units (at most 1,000 under the default rules) fit the 16,384-unit message
bound. A camera containing more than that many foreign units still receives
the explicit 413 refusal; this cap change does not qualify such crowded views.

The capacity tests exercise housing through 1,000 and reservation of the last
slot, plus the 100-seat storage arithmetic. The historical runs below used cap
200; they do not demonstrate 100,000-unit RTS performance.

## Per-tick and per-order cost

The capacities are bounds, not allocations: memory and tick cost follow what is
actually built. Whole-world scans removed on the order path: housing on every
placement, the build requirement check, and the builder and hunter capacity
checks now read one player's buildings (the per-player, per-kind index) or the
order's own group. `advance_training` visits only buildings with a queue.
Training spawn occupancy and placement occupancy now use spatial buckets and
the existing full-footprint building tile index, including Farms and TC arches.
The Vassal phase has [fixed admission budgets](../economy/vassal.md), separate
from the ordinary shared route budgets.
Remaining O(buildings) per-tick scans: research, gates, the watched-building
memory refresh. In a debug build 13,600 buildings and 31,000 collision tiles
advance 20 ticks in about 120 ms; the long match below gives release numbers.

## Long match

"World capacity run" in [the match results](match.md): the owner's setup (France package, 99
bots, cap 200) for 24.5 game minutes held 0.98x real time (tick p50 25.4 ms, p95 51.3 ms) with
1,591 bot buildings, 77 bots in Feudal Age, and no build order refused as a world limit or a full
queue.
