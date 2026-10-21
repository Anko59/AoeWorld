# AI SDK

Multiplayer games against AI opponents written as ordinary Python projects.
An AI is a **separate process** that connects over the same gameplay WebSocket
as the browser, sees only what its fog of war allows and sends the same orders a
person can. It cannot cheat: nothing in the SDK reads simulation state, and the
server filters every message per slot. One Python host process can run many
bots; the target is one human and 99 AIs on one server.

```
 browser ──┐                                   ┌── aoe-server ───────────────┐
 (1 human) ├─ /game/ws  Hello | JoinSlot ─────►│ slot 0  slot 1 ... slot 99  │
 Python    │            postcard, per-slot     │  player 0  player 1 ...     │
 host ─────┘            fog-filtered views     │  one GameWorld, 20 Hz       │
 aoe-ai: Fleet → Client ×N (one Rust runtime)  └─────────────────────────────┘
```

Layers, each testable on its own:

| Layer | Where | What it adds |
| --- | --- | --- |
| Server | `crates/server` | slots, per-slot auth, fog views, `POST /match` |
| Rust SDK | `crates/sdk` (`aoe-sdk`) | typed world model, every order, events, bot loop |
| Python bindings | `crates/sdk-python` (`aoeworld`) | the SDK for Python, GIL released while waiting |
| Example AI | `ai/` (`aoe_ai`) | a Dark and Feudal Age player and a many-bot host |

## Slots, tokens and what a slot sees

A match has *N* slots numbered from 0; slot *i* controls player *i* and nothing
else. Every slot has a 192-bit token (48 hex digits) created with the match.

* `JoinSlot { version, token }` (client message variant 19, appended after the
  browser's variants and pinned by a golden in `crates/protocol`) authenticates
  for exactly that slot. An unknown token is answered with `Error 403` and the
  socket closes: it is never demoted to a spectator and never takes a free
  human seat. Presenting the token again replaces the old connection (reconnect).
* `Hello { version, resume_token }` is unchanged, so the browser keeps working:
  with a slot token it resumes that slot, otherwise it claims the first **human**
  slot nobody holds, otherwise it spectates (watching slot 0's view). AI slots
  are never reachable without their token. A human slot whose controller stays
  away 30 seconds goes to the oldest spectator under a fresh token.
* A controller's commands apply to its player only: an order naming another
  player's unit is answered `RejectedNotOwner`. Snapshots, tick deltas, the
  fog state, wildlife, projectiles and resource amounts are computed per
  slot ([fog of war](fog-of-war.md)); another player's economy never appears.
  Enemy buildings arrive as fog memory.
* AI slots get a larger order budget (100 orders a second, burst 200 against a
  human's 10 and 20), no idle `Tick` messages (a heartbeat every half second
  keeps the clock moving) and a fog view rebuilt at most twice a second.
  Tokens of AI slots never authorise server administration (map activation).

## Starting a match

`POST /match` replaces the running game. It is refused unless the server was
started with `AOE_MATCH_TOKEN` (at least 16 characters) and the request carries
the same value in `X-AoeWorld-Admin-Token`.

```json
{"flat": {"tiles_per_side": 500, "seed": 3}, "slots": ["human", "ai", "ai"]}
```

Name the ground with exactly one of:

* `"flat": {"tiles_per_side": 64..4096, "seed": n}` a flat synthetic ground: one
  elevation, so only trees, bushes and ore obstruct movement. This is the place
  to develop and test an AI (generated terrain without source data is rugged,
  and its cliffs wall off most village sites). Needs the server's map package
  directory (`AOE_MAP_PACKAGE_DIRECTORY`, default `local-assets/map-packages`).
* `"map": {MapRequest}` a generated package (the browser's map creator
  request); `"map_hash": "<64 hex>"` a package the server already holds.

Villages are placed on a ring (up to 8 players) or a grid (more) spread over the
whole map, each near its ideal point, on ground that connects to a large walkable
region where the terrain has one, and kept apart. Give each player room: about
40x40 tiles (a 100-player match wants 800 or more tiles per side).

The response lists each slot: `slot`, `player`, `kind`, `token`,
`primary_unit_id` and `start_tile`. Give an AI slot's token to its process; the
browser's human slot needs no token (a plain `Hello` claims it).

```
AOE_MATCH_TOKEN=change-me-0123456789 AOE_BIND=127.0.0.1:9000 aoe-server
python -m aoe_ai match --url http://127.0.0.1:9000 --admin-token change-me-0123456789 \
    --humans 1 --bots 99 --tiles 900 --minutes 20
```

## Writing an AI in Python

```python
import aoeworld
from aoeworld import UnitKind, ResourceKind

client = aoeworld.connect("http://127.0.0.1:9000", slot_token, decision_hz=2)

def on_tick(client, decision):            # called every decision point
    world = decision.world                # an immutable snapshot
    for villager in world.idle_units(UnitKind.VILLAGER):
        node = world.nearest_resource(ResourceKind.WOOD, *villager.tile)
        if node:
            client.gather(villager.id, node.id)
    for event in decision.events:         # what happened since last time
        if event.kind == "under_attack":
            ...

client.run(on_tick, until_tick=20 * 60 * 10)   # ten game minutes
```

* **Decision rate.** `decision_hz` decision points per game second (the game runs
  20 ticks a second, so `decision_hz=2` is every 10 ticks). Decisions are tied to
  the game clock, not the wall clock, and a slow bot skips points instead of
  queueing them. `client.run(on_tick)` loops; `client.wait_decision(timeout)`
  does one step (the wait releases the GIL, so other threads run); `aoeworld.Fleet`
  waits for any of many clients and returns every due decision, so one thread
  drives a hundred bots; `aoeworld.AsyncFleet` is the same for asyncio.
* **Orders** are methods of the client and return a sequence number; the
  server's verdict arrives as an `order_result` event carrying that number and a
  `CommandResult` (`ACCEPTED` or a `REJECTED_*` reason). Orders are applied on the
  next server tick, so a snapshot taken right after sending does not show them.
* **The world** holds own units, buildings and economy, visible foreign units,
  remembered foreign buildings (`visible` says whether in sight), wildlife,
  explored resource nodes, the fog (`visibility(x, y)`), walkable tiles
  (`passable`) and a building-site finder. Nodes appear only on explored tiles:
  scout. Resource amounts are the last ones seen.
* **The view window.** The server describes foreign units, fog and wildlife only
  inside the slot's subscribed region; own units are always reported. The SDK
  moves a window (`view_size`, `view_padding`) over the slot's non-scout units
  and buildings so a bot pays only for what it can act on.
* **Rules tables.** `aoeworld.building_cost`, `unit_cost`, `tech_cost`,
  `trainable`, `build_seconds`, `building_size`, `building_requires` expose the
  numbers the server charges from.

### Events

`unit_spawned`, `unit_died` (`mine` tells whose), `building_completed`,
`building_destroyed`, `research_completed`, `age_advanced`, `under_attack`
(`id`, `attacker`, `is_building`, `x`, `y`), `order_result`, `server_error`,
`world_reset` (the server started another match: reconnect). They are derived
from the deltas the slot receives, so they cover exactly what it can see.

## Python API reference

`aoeworld.connect(url, slot_token, decision_hz=2.0, view_padding=28, view_size=128,
fetch_terrain=True) -> Client`; `aoeworld.start_match(url, admin_token, slots,
tiles_per_side=500, seed=1, terrain="flat", map_hash=None) -> MatchInfo`. Exceptions: `AoeError`,
`ConnectionClosed`, `Refused` (bad token, bad secret).

`Client`: `wait_decision(timeout=None) -> Decision | None`, `run(on_tick,
until_tick=None, max_decisions=None) -> RunSummary`, `world`, `closed`,
`close_reason`, `close()`, and the orders `move(unit, x, y)`, `stop(unit)`,
`gather(unit, resource_id)`, `build(builders, kind, x, y)`, `build_wall(builders,
x1, y1, x2, y2)`, `construct(builders, building)`, `cancel_construction(b)`,
`farm(unit, farm)`, `train(building, kind)`, `cancel_training(b)`,
`research(building, tech)`, `cancel_research(b)`, `attack(unit, target)`,
`hunt(units, animal)`, `market_buy(building, resource)`, `market_sell(...)`.

`World`: `tick`, `tick_hz`, `seconds`, `map_size`, `me`, `economy`,
`my_units(kind)`, `idle_units(kind)`, `visible_enemies()`, `unit(id)`,
`my_buildings(kind, complete_only)`, `known_enemy_buildings()`, `building(id)`,
`town_center()`, `unfinished_buildings()`, `animals()`, `resources(kind)`,
`nearest_resource(kind, x, y)`, `visibility(x, y)`, `passable(x, y)`,
`find_build_site(kind, x, y, max_radius, gap)`.

`Unit`, `Building`, `ResourceNode`, `Animal`, `Economy` (stockpile, population,
`age`, `researched`, market prices, `can_afford`) and `Event` are read-only
objects; enumerations are `UnitKind`, `BuildingKind`, `ResourceKind`, `Tech`,
`Age`, `UnitTask`, `TradeResource`, `AnimalKind`, `AnimalTask`, `Visibility`,
`CommandResult` (`UnitKind.VILLAGER`, `Tech.FEUDAL_AGE`, ...). Types are in
`python/aoeworld/_native.pyi`, checked against the module by the test suite.

## Rust SDK reference

`aoe_sdk::Client::connect(url, token, ConnectOptions)` joins a slot;
`next_decision()` yields `Decision { world, events }`; `send(Order)` returns the
sequence number; `Order` has one variant per browser order (move, gather, build,
wall, construct, farm, train, research, attack, hunt, market buy and sell, stop,
and the three cancels). `run_bot(&client, &mut bot, RunLimits)` runs a `Bot`
(`decide(&Decision, &mut Vec<Order>)`) at the decision rate. `start_match` calls
`POST /match`. `World` mirrors the Python queries; `rules` holds the cost tables.

## The example AI (`ai/`)

`aoe_ai.Brain` is the player; `Host` runs many in one process. Each decision it
runs these in priority order, sharing one `Context` (the snapshot plus a spending
budget) and a `BotMemory`:

| Module | Job |
| --- | --- |
| `research.py` | Loom, the climb to Feudal Age (drains the Town Center queue first), then economy upgrades |
| `combat.py` | defend the base, mass 14 units, attack the nearest known enemy building, retreat below 40% |
| `economy.py` | villager production, resource shares (food, wood, gold, stone), hunting before foraging, farms once food nodes run out |
| `constructor.py` | houses before the cap, lumber camps, mining camps and mills beside far resources, Barracks, Archery Range |
| `military.py` | spearman : archer : skirmisher = 4 : 3 : 3, rally point |
| `scouting.py` | widening rings, then a sweep of the far side of the map |

It learns from refusals (a refused node or site is not tried again). Tunables
are in `config.py`. `python -m aoe_ai --help` lists the launcher modes.

## Tests and commands

* `make sdk-python` builds the wheel (`target/wheels`); `make sdk-python-test`
  builds the server and a wheel, then runs `pytest` for the bindings and the
  example AI against a local server. Both run in the pinned
  `aoeworld/sdk-python` image (`docker/sdk-python.Dockerfile`).
* `cargo test -p aoe-sdk` (through `make test-unit`) tests the model, orders and
  two bots on a local server; `crates/server/tests/slots.rs` and
  `match_http.rs` test slot isolation, per-slot fog and `POST /match`.

Limits: the 100-player target is a capacity and design target; this document
does not claim finished RTS performance at that scale. See
[architecture](architecture.md) for the server and [performance](performance.md)
for what is measured.
