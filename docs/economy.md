# Village economy

The authoritative rules live in
[`VILLAGE_RULES`](../crates/simulation/src/game_world/economy.rs).
It is the current gameplay table for starting resources, villagers, footprint,
population support, villager movement/carrying/gathering and training.
The scout retains the existing world's configured movement speed.

Map activation first runs the existing certified start search, then performs
bounded local placement for the Town Center and free perimeter spawn tiles.
The scout keeps the primary unit id. Site ranking prefers inland footprints
with real trees within twelve tiles: at most 384 fine/grid sites and 12,000
memoized map samples within the existing 32-tile search radius. Remaining
footprints form a deterministic bounded fallback. Placement preserves the scout's
found position when it is within eight tiles of the chosen center; otherwise it
starts on the perimeter. Footprints cannot cover
units, resources, water, other buildings or the map boundary. They participate
in map route planning, passability and diagonal collision.

Map starts top up reachable resources within fifteen tiles to seven berry
bushes, seven gold nodes, five stone nodes and twelve trees. Compact clusters
prefer offsets of six, nine, ten and eight tiles respectively. A cardinal graph
within fifteen tiles certifies gathering access to every new and existing node
and preserves all center exits after each cluster. Each kind tries at most 128
anchors in two orientations; no terrain or natural resource is removed. Each
kind tries its full cluster, then one node, and is omitted when neither fits, so
a start with no room for a kit begins with an empty or partial one. Activation
is refused when no footprint has enough reachable free perimeter tiles for the
starting villagers, plus the scout when it must leave its found position.
Restored overlays retain the original kit, including exhausted nodes.

Starting resources are authoritative overlay entries: bit 37 marks their
identity, bits 38–39 select the kind, and the remaining coordinate fields match
natural resource identities. Initial entries do not increment the depletion
revision. Snapshots carry their complete amounts; normal deltas carry depletion.
The client derives their existing sprite family from their kind. The overlay
blocks their tiles and includes them in resource lookup, persistence and replay.

Gather commands require an owned villager and a live map or starting resource
node. A villager walks to a passable adjacent tile and earns integer gathering
credits from game ticks. Each complete resource unit uses the existing prepared
depletion transaction. Stored-map services persist that transaction before
committing it or adding to the carried load. A refused save discards the failed
attempt's credits and pauses retries for one game second.

Full loads and loads from exhausted nodes return to the nearest reachable owned
Town Center perimeter. Deposits change only the matching resource stockpile.
The job resumes the same node; after exhaustion it searches for a same-kind
node within the rules table's Euclidean radius around the exhausted node.
Without a replacement or after a terminal resource-route failure it becomes
idle. A return trip remembers failed perimeter tiles and tries other owned
drop-off tiles. Exhausting them ends the job while preserving the carried load.
Without an owned drop-off yet, it keeps the load and retries on game time.
A deposit or a new order clears the trip's failed choices. Changing resource
kind discards the carried load; a plain Move cancels the job while keeping the
load.

Train commands require an owned, completed building that trains the unit, enough resources, a free population
slot and space in its bounded queue. Acceptance pays the unit cost and reserves the
population slot; population used continues to count living units. Only the
front entry advances. A completed entry waits for a free passable perimeter
tile and does not pay again. New villagers use the shared entity allocator.
Cancelling the active training entry refunds its cost and releases its reserved
population slot; cancelling an empty queue cannot create resources. Death, rally
points and saved village state are not part of the economy.

Gameplay protocol 2 is the only current wire. Gather, Train and CancelTraining share Move's
bounded per-session/global command queue, sequencing and role checks. Snapshot
and tick subscription revisions also cover player stockpile/population and
building footprint/queue/progress. Per-session residents produce sparse changes;
failed delivery disconnects rather than losing a state delta. Resource revisions
continue through the independent bounded overlay feed. Reconnect/resync sends
complete current state. Wire collection and value bounds live in the protocol
crate; changing a gameplay value beyond those bounds also changes that current
wire contract and its golden fixtures in place.

The uniform compatibility start has stockpiles; immutable-map resource nodes
are available after activating a resource-bearing map. The [village interface](village-interface.md) presents current player/building
state and dispatches these commands. The server remains the economy authority.

Focused validation is `make duel-check HARNESS_DUEL_BASE=<merge base>` followed
by `make test-wasm` and `make test-e2e`. Economy tests cover exact tick rates,
all resource deposits, depletion/replacement, persistence refusal, training,
placement/collision, replay, wire fixtures/bounds and service lifecycle. The
headless test records its observed deposits and training in
`.cache/economy/headless.log` for the local narrated demonstration.
Synthetic evidence does not qualify finished RTS performance or hardware.

## Dark Age buildings

Every building value lives in one table,
[`BUILDING_RULES`](../crates/simulation/src/game_world/economy/buildings.rs)
(cost, build seconds, hit points, size, line of sight, housing, accepted
resources, trainable units, requirement). Fix a number there and the rules,
tests and wire follow. Values are Age of Empires II: Definitive Edition's.

| Building | Cost | Time | HP | Size | Role |
|---|---|---|---|---|---|
| House | 25 wood | 25 s | 550 | 2x2 | +5 population |
| Lumber Camp | 100 wood | 35 s | 600 | 2x2 | wood drop-off |
| Mining Camp | 100 wood | 35 s | 600 | 2x2 | gold and stone drop-off |
| Mill | 100 wood | 35 s | 600 | 2x2 | food drop-off |
| Farm | 60 wood | 15 s | 480 | 3x3 | 175 food, needs a Mill, walked on |
| Barracks | 175 wood | 50 s | 1200 | 3x3 | training queue (no unit kind yet) |
| Palisade Wall | 2 wood a tile | 6 s a tile | 250 | 1x1 | blocks movement |
| Outpost | 25 wood, 5 stone | 15 s | 500 | 1x1 | line of sight 10 |

Build and BuildWall orders name villagers and a tile. The server validates the
footprint (inside the map, passable, free of resources, units and buildings),
the owner, the requirement and the cost, then charges the cost and creates a
foundation (`ConstructionState::UnderConstruction`, hp 1 growing with progress).
A foundation blocks movement like the finished building (a farm never does).
Builders walk to a tile beside it; each tick the foundation gains `builders + 2`
of `3 x base seconds x tick rate` work, the AoE II rule `base x 3 / (n + 2)`: one
builder takes the base time, three take three fifths of it. Builders stop on
completion; beside a new camp or mill they gather the nearest matching resource,
the builder of a farm farms it, and a palisade builder continues to unfinished
segments within eight tiles (the nearest four are tried). A Move order
abandons construction; `ConstructOrder` sends villagers back to an unfinished
foundation the player owns, at no charge. Cancelling a
foundation refunds the cost in proportion to the work not yet done (all of it
before any progress). One palisade order lays at most 64 segments, all or none.

Population support is the sum over a player's completed housing buildings
(Town Center 5, House 5), capped at 200. Training needs a completed building
whose table lists the unit (`trainable_units`); the Barracks trains Militia
(60 food, 20 gold, 21 s, from the combat table; cancelling refunds both). A returning villager walks to the nearest completed
owned building that accepts its load (`nearest_dropoff`).

A farm holds 175 food from placement, but only a completed one can be worked,
by one villager standing on it (`UnitTask::Farming`, food rate as gathering). It
returns each 10-food load to the nearest food drop-off, and when the 175th
unit is delivered the farm building is removed (a removal on the wire). A farm
with no food left is also removed when its farmer is ordered away (Move, build,
farm or gather) before delivering; a farm with food left stays workable.
Per-tick cost is bounded by builders and gatherers (at most 80 each) and the
building count (at most 256); placement and completion alone scan buildings.

Wire: `BuildOrder`, `BuildWallOrder`, `ConstructOrder`, `CancelConstructionOrder` and
`FarmOrder`;
each building state carries `construction`, `progress_permille`, `hp`, `max_hp`,
`los` and `food_remaining`; `UnitTask` gains `Building` and `Farming`. Hit
points are authoritative state for the combat track to change.

Start activation also surveys the whole map on a bounded coarse lattice,
ranking connected sampled lowlands before elevation and centre distance.
On low-relief maps, an already certified inland center can return early after
an elevation-only survey of the same whole-map lattice. Its resource-aware
clearance, reachable-area and exit certificates remain mandatory. Mountainous
maps retain the connected lowland search.
Shore avoidance, gentle slopes and reachable area relax in stages; every
returned start remains the center of a resource-aware clear area of at least
5×5 tiles. A cancellable exact fallback checks at most every possible 4×4
origin, requiring that clearance and room beside the footprint for units. The
coarse lattice is a preference, not a claim of a certified route between samples.

Natural ore patches use one jittered cluster per 384×192 tiles for gold and
192×96 for stone (at most eight candidates per cell, before terrain/access
filtering). Decorative rocks retain relief influence, with less lowland dirt
clutter and small loose-rock art; they carry no resources. Deadwood dressing
uses cut stumps and fallen logs, excluding upright snag art. Every live
resource blocks navigation; gathering stands beside it, and a zero remaining
amount removes the sprite and collision, including after reconnect.

Source river station intervals are not crossing evidence. Rendered river
water is impassable; no bridge or ford is inferred from those intervals.

## Technologies and ages

Every technology lives in one table,
[`TECH_RULES`](../crates/simulation/src/game_world/research/table.rs): the
researching building, cost, seconds, requirement and effect. A player's `age`
(Dark, Feudal) and `researched` bitset (`1 << tech`) are player state on the wire;
each building carries its `research` (tech, progress, total ticks).

| Tech | Building | Cost | Time | Effect |
|---|---|---|---|---|
| Loom | Town Center | 50 gold | 25 s | villagers +15 HP, +1 melee and +2 pierce armour |
| Town Watch | Town Center | 75 food | 25 s | building line of sight +4 (`los`) |
| Feudal Age | Town Center | 500 food | 130 s | age becomes Feudal; needs 2 completed types of Barracks, Mill, Lumber Camp, Mining Camp |

`ResearchOrder { building_id, tech }` pays the cost at once and occupies the
building: it must be complete, owned, able to research the tech, with an empty
training queue and no research; a building researching refuses training. A
technology is researched once, and a player runs one research of it at a time
(a second Town Center is refused the same tech meanwhile). `CancelResearch`
refunds the whole cost. Houses, walls and outposts never count for the Feudal
Age; Docks count alongside the other economic and military buildings. Loom raises `max_hp` and
`hp` together, so a damaged villager keeps the damage taken; new villagers
start with it, and `GameWorld::unit_armour` adds it to the combat table's armour.
Town Watch raises `los` of existing and later buildings. Feudal unlocks nothing
beyond the age field. Work per tick is bounded by the building count.

Dark Age building melee/pierce armour from AoE II DE: House -2/7; Mill,
Lumber Camp, Mining Camp and Barracks 0/7; Palisade Wall 2/5; Outpost and
Farm 0/0. Town Center remains 3/5. Negative melee armour increases damage.
## Wildlife and hunting

`WILDLIFE_RULES` in `game_world/wildlife/table.rs` is the animal data table:
sheep 7 HP / 100 food, deer 5 / 140, boar 75 / 340 / 7 attack,
wolf 25 HP / 3 attack / no food. Shepherds gather at 0.33 food/second;
hunters at 0.41, compared with foragers' 0.31. These are the DE values used
by this implementation; no original game assets are needed.

`HuntOrder { unit_ids, animal_id }` assigns owned living villagers. Hunting
uses 3 damage with a two-second period, at one tile for sheep and four tiles
for bow hunting. A new command
replaces the hunt. Death cancels the worker's job. Dead animals keep a body
for one second, then remain carcasses. Hunters automatically gather their
food and deliver ten-food loads through the normal reachable food drop-off
selection: owned completed Town Centers or Mills only. Sheep use Shepherding;
other animals use Hunting. Unattended carcasses decay at 0.25 food/second
(sheep/deer) or 0.40 (boar). A worker assigned to the carcass, including its
return trip, keeps it attended; decay resumes when the last job is cancelled.

Neutral sheep convert within two tiles to the nearest living unit's player;
ties use player then entity id. Owned sheep stay owned. Deer flee within
four tiles at 1.0 tile/second, versus villagers' 0.8. Boar retaliate against
the last villager to hit them and stop chasing beyond eight tiles of home.
Wolves acquire villagers within six tiles. Movement is fixed point and tests
terrain, diagonal crossings and building collision; animals do not block tiles.
Animal acquisition uses distance without occlusion. Its `sight_tiles` is an AI
range, separate from the unit's fog `los_tiles`. Hunt targets are explicit;
combat AttackOrder targets live units, buildings and animals. Owned sheep
accept MoveOrder for herding.

The map generator has no animal hooks. Village initialization therefore places
a deterministic local kit of eight sheep, two boar, four deer and one wolf, searching
bounded local rings for free passable tiles. Constrained starts may omit an
animal if no tile fits.

Live animals occupy an eight-tile spatial index. Each tick probes a fixed
neighborhood around living units and advances only candidate animals and
active chases/flights (including target cleanup), plus bounded hunt jobs (80).
Distant idle animals add no tick work, verified
with equal counters for 10 and 1,000 distant sheep. Carcass lifecycle/decay uses
an ordered due-time index, so only due carcasses are visited. Storage is capped
at 4,096 animals. Replay hashes include animal state, cooldowns, fractional
movement/decay, due times and hunt jobs.

The appended `WildlifeState` wire variant carries the complete, bounded animal
set for the subscription region at its revision and tick; it replaces the
previous set, including removals on panning. Subscribe/resync always sends it;
ticks send it when that set changes. `HuntOrder` and worker task variants are
appended, preserving existing enum indexes and golden fixtures. The headless
wire scenario writes `.cache/wildlife/scenario.log`. The client uses the current
[native unit and wildlife animations](assets/units.md) for combat and hunting.

Fog is authoritative player knowledge; see [fog of war](fog-of-war.md) for
storage, observation rules, wire bounds and source values.
Dark Age [fishing](fishing.md) adds shore/deep fish, the Dock and naval Fishing
Ships. The local DAT sources, rates, bounded water routing, append-only wire
values and reproducible coast scenario are documented there.
