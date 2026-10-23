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
  keeps the clock moving) and a fog view rebuilt at most once a second.
  Tokens of AI slots never authorise server administration (map activation).

## Starting a match

`POST /match` replaces the running game. It is refused unless the server was
started with `AOE_MATCH_TOKEN` (at least 16 characters) and the request carries
the same value in `X-AoeWorld-Admin-Token`.

```json
{"flat": {"tiles_per_side": 500, "seed": 3}, "slots": ["human", "ai", "ai"]}
```

Optional match rules (both validated, defaults keep the shipped game):

* `"population_cap": 5..5000` (default 200): the ceiling on any player's
  population; the world's capacities are sized per seat from it (see
  [capacities](scale/capacity.md)). 100 players x 1,000 = 100,000 units.
* `"start_units": {"villagers": n, "militia": m}`: extra units every player
  starts with beside the standard kit, placed around the village. A scenario
  hook for tests and [scale proofs](scale/match.md); the response reports
  `population_cap` and `seeded_units`.

`GET /match/stats?since=n` (same secret) returns the per-tick records of the
server (`[tick, total_us, sim_us, late_us, orders, units, missed]`) that
`make scale-match` reads.

Name the ground with exactly one of:

* `"flat": {"tiles_per_side": 128..4096, "seed": n}` a flat synthetic ground: one
  elevation, so only trees, bushes and ore obstruct movement. This is the place
  to develop and test an AI (generated terrain without source data is rugged,
  and its cliffs wall off most village sites). Needs the server's map package
  directory (`AOE_MAP_PACKAGE_DIRECTORY`, default `local-assets/map-packages`).
  Natural resources come from the procedural landscape (forests, a few berry
  bushes), but every village also gets the standard start (see below), so
  each has gold, stone, sheep, deer and boar. A map under about 300 tiles can
  have few trees besides the start's twelve.
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

## Skirmish from the browser

The setup screen's **Opponents** setting (0 to 99) plays a match against the
example AI without any command line. `make dev` builds the wheel, installs it
into `target/dev-ai/site` and runs the server in the SDK image with
`AOE_AI_COMMAND="python3 -m aoe_ai host"`. After the map job, the browser calls
`POST /skirmish {"map_hash", "opponents"}` (controller token): the server
builds the match (slot 0 human, the rest AI, standard starts, population cap
200), swaps it in and starts one host process per 13 AI slots, handing each
its tokens on stdin. `GET /skirmish` reports the phase (`opening_map`,
`placing_villages`, `starting_ai`, `connecting_ai`, `ready`, `failed`,
`cancelled`, `stopped`) with measured counts. The hosts are killed by
`POST /skirmish/stop` (Cancel, Quit to Main Menu), when another match or map
replaces this one, when they exit, or when nobody has held the human seat
for five minutes.

```sh
AOE_PRESET_MAPS=qa/skirmish-presets.json DEV_PORT=8097 make dev   # France prepared at boot
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

Production buildings retain a server-owned rally point across reconnects. In
Python, use `set_rally(building, x, y)`, `set_rally_resource(building, resource)`,
`set_rally_building(building, target)` or `set_rally_animal(building, animal)`;
these return ordinary order sequences. Buildings expose `rally_position` and
optional `rally_resource`, `rally_building` and `rally_animal` identities. Rust
uses `Order::SetRally` with `RallyTarget` and observes `Building.state.rally_point`.
Known resources may be in explored fog; undiscovered targets are refused.

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
  scout. Exploration is remembered for the whole map from the server's 256x256
  strategic overview (coarse) and tile-exactly inside the view window. Resource
  amounts are the last ones seen.
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
fetch_terrain=True, orders_per_second=90) -> Client`; `aoeworld.start_match(url, admin_token, slots,
tiles_per_side=500, seed=1, terrain="flat", map_hash=None) -> MatchInfo`. Exceptions: `AoeError`,
`ConnectionClosed`, `Refused` (bad token, bad secret).

`Client`: `wait_decision(timeout=None) -> Decision | None`, `run(on_tick,
until_tick=None, max_decisions=None) -> RunSummary`, `world`, `closed`,
`close_reason`, `orders_sent`, `close()`, and the orders `move(unit, x, y)`, `stop(unit)`,
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
builds one `Context` (the snapshot, a spending budget, order helpers) and runs
the managers in priority order, sharing one `Blackboard` (orders in flight and
their verdicts, refused nodes and sites, planned buildings, scouting progress,
attack state, counters):

| Module | Job |
| --- | --- |
| `play/buildorder.py` | the plan: stage (Dark, advance, Feudal), when to Loom and click Feudal Age, which buildings and upgrades come next |
| `play/research.py` | executes it: Loom, saving 500 food and Feudal Age (the Town Center queue drains first), then the upgrades |
| `play/economy.py` | villagers without pause, shares of food, wood, gold and stone, sheep then berries then deer and boar, farms when the land runs dry |
| `play/constructor.py` | houses before the cap, Lumber Camp and Mill, camps beside far resources, Barracks, Archery Range, Blacksmith |
| `play/military.py` | spearman : archer : skirmisher = 4 : 3 : 3 from Feudal Age on, rally point |
| `play/scouting.py` | the starting area, then the circle round the map centre through home (far side first), then wider rings |
| `play/combat.py` | defend the base (villagers next to raiders walk home when outnumbered), attack the nearest known enemy building at 12 units, retreat below 40% |

The build order: villagers from the first second, a House before the cap, a
Lumber Camp and a Mill at 8 villagers (they also satisfy the age's two-kinds
rule), Loom at 10, and at 21 villagers with those standing it stops training
villagers, saves 500 food and researches Feudal Age. While the age researches
it raises the Barracks; in Feudal Age the Archery Range, the Blacksmith, Double-Bit
Axe, Horse Collar (once farming), Wheelbarrow, Gold Mining and the Blacksmith
upgrades, and an army from every Barracks and Range. No unit is trained in the
Dark Age: its food belongs to villagers and the age.

Robustness: every order is remembered until its verdict arrives; a refused
node, animal or site is never tried again, a refused unit kind is substituted
until the age changes, a villager that falls idle right after an order waits
four seconds, and a hunted animal nobody finishes is dropped. Everything is
read from the fog-filtered world: nothing is assumed about enemy positions
beyond "villages are spread evenly over the map".
Tunables are in `config.py`. `python -m aoe_ai --help` lists the launcher modes.

### The standard start

Every village of a match starts like the Arabia start of Age of Empires II:
Town Center, 3 villagers and a scout, 8 sheep, 7 berry bushes, 7 gold and 5
stone nodes, 12 trees (topped up beside the natural ones), 2 boar, 4 deer and a
wolf, placed by the simulation's deterministic starting kit
([economy](economy.md), [wildlife](economy.md#wildlife-and-hunting)). The
official guide lists, per player, 8 herdables, 3 forests, 6 fruit bushes, 2
aggressive and 4 wild hunt, 7 tiles of gold and 5 of stone
(<https://ageofempires.com/learn-to-play/defending-your-empires-aoe2>). A bug
gave only the first village of a match its kit (the guard looked for any
starting node in the world); `gameplay_match` tests now check every village of
several flat matches. These nodes are in no map chunk, so the SDK builds them
from the resource feed's ids (`World.resources()`); before that fix bots never
saw their berries and gold.

### The AI arena

`make ai-arena` plays seeded matches as fast as the machine allows
(`ARENA_SIZE`, `ARENA_BOTS`, `ARENA_MINUTES`, `ARENA_SEEDS`; defaults 300 tiles,
2 bots, 30 game minutes, seeds 1-5) and prints JSON: per seed and bot, the Feudal
minute, villagers (and peak), army (and peak), attacks launched and landed,
losses and kills. A server started with `AOE_ARENA=1` has no tick clock; the
driver (`ai/aoe_ai/arena.py`) advances it with the admin-token call
`POST /match/step {"ticks", "orders"}`. The bots are ordinary clients: same
protocol, same SDK, same fog-filtered view, and `decision_hz` counts game
seconds. The driver steps one decision period, waits until every bot has its
decision, lets them decide, then steps again; the step waits for the orders the
bots sent, and the real-time order budgets (server and client) are lifted. A
seed repeats exactly in practice (same result twice on this machine); the
exceptions are two benign races, a bot whose map window moved during a step (the
driver then adds a tick) and wildlife updates reaching a bot within a few
milliseconds of its poll. A game finishes in a fraction of its length, but the
wall time still grows with the armies and the machine's load.

### Measured arena play

`ai/tests/test_full_game.py` plays seeds 1-5 on a 300-tile flat map, two bots, up
to 30 game minutes, and requires per bot: Feudal Age, at least ten soldiers at
once (`SOLDIERS`) and, per match, one landed blow (an enemy's health fell with
our soldiers within 12 tiles). `sdk-python-test`, which blocks `ci` and
`nightly`, runs it because all five seeds passed on the revision that
introduced it (development machine, shared, so wall times only indicate):

| seed | Feudal minute (bot 0 / 1) | army peak (0 / 1) | blows landed (0 / 1) | wall |
| --- | --- | --- | --- | --- |
| 1 | 12.5 / 12.4 | 50 / 60 | 1 / 65 | 880 s |
| 2 | 12.1 / 12.0 | 66 / 47 | 95 / 0 | 1078 s |
| 3 | 12.5 / 12.1 | 25 / 56 | 0 / 135 | 307 s |
| 4 | 12.7 / 12.4 | 38 / 17 | 122 / 0 | 233 s |
| 5 | 12.4 / 12.4 | 33 / 14 | 67 / 0 | 222 s |

Full 30-minute games; the test stops a seed as soon as it is proven, and wall
time grows with the armies and the machine's load (two arenas ran side by side
here). The weakest bot of any seed peaked at 14 soldiers, so ten is met with a
margin, but five seeds are a sample: if a later change breaks the gate, fix the
AI or the threshold, and move the test to a non-blocking cadence rather than
tolerating a flaky blocking gate. Reasons the earlier live runs showed thin
armies: foundations whose builder was lost stayed unbuilt for ever (the Archery
Range and second Barracks never stood; the AI now sends a villager to
`construct` a foundation that stopped advancing), too much gold and too little
food for a Feudal army (shares moved to food), and hunters held by animals they
could never kill (now stood down after 40 s without progress). The Feudal time
(about 12.5 minutes) is later than a good player's. Synthetic two-bot workloads
on a flat map show neither finished RTS performance nor dedicated-hardware
qualification.

`python -m aoe_ai host --tokens-file` plays many existing slots in one process and writes a
JSON report with per-bot drop and rejoin counters (the scale harness uses it).

## Tests and commands

* `make sdk-python` builds the wheel (`target/wheels`); `make sdk-python-test`
  builds the server and a wheel, then runs `pytest` for the bindings and the
  example AI against a local server. Both run in the pinned
  `aoeworld/sdk-python` image (`docker/sdk-python.Dockerfile`).
* `cargo test -p aoe-sdk` (through `make test-unit`) tests the model, orders and
  two bots on a local server; `crates/server/tests/slots.rs` and
  `match_http.rs` test slot isolation, per-slot fog and `POST /match`.

## Scale: one human and 99 AIs

Design: a compact binary protocol (postcard, no JSON on the hot path); an
interest-limited view per connection (own units always, foreign units, fog and
wildlife only in the bot's window); per-player vision revisions and per-viewer
economy and building views built once per tick; idle `Tick` messages dropped
for AI slots (a heartbeat every half second); AI fog views at most once a
second; one Rust runtime with two network threads for every connection of a
Python process; `Fleet.wait` serving all bots from one Python thread. Capacities
are sized per seat: 1,024 buildings and one gatherer, builder and hunter per
unit of population for every seat ([capacities](scale/capacity.md)). A refused
order says why: `REJECTED_WORLD_LIMIT` when a world capacity is reached,
`REJECTED_ORDER_TOO_LARGE` for an oversized group or wall, and
`REJECTED_QUEUE_FULL` only for a full order queue.

Measured on the development machine (12 cores, shared with other work, release
server, flat 900-tile map, `aoe-ai` bots at 2 decisions a second, one Python
process):

| | 30 AI + passive | 99 AI + 1 human slot, playing |
| --- | --- | --- |
| match creation (`POST /match`) | 28 s | 76-98 s |
| all bots connected | under 1 s | 2.5-3.9 s |
| server CPU for the sessions alone | 0.05 core, no missed ticks | |
| 60 game seconds | | 91 s wall, server 1.0 core, Python 0.29 core |
| errors, closed connections | | 0 |

Reading it honestly: connecting and serving 100 slots is cheap (the networking
layer is not the bottleneck: 30 connected passive clients cost 0.05 core), but
with 99 AIs issuing orders the single-threaded simulation tick did not keep
20 Hz here (619 missed deadlines in 1,200 ticks, 0.66x real time); that cost is
movement planning and gathering, which the simulation-scale work owns. Match
creation spends about a second per village on site certification. The run
covers a young economy (about four villagers each), so it demonstrates
capacity and connection handling, not finished RTS performance or dedicated
hardware qualification. Ad hoc scripts produced the numbers; they are
not a gate.

Limits: capacities and design are for 100 players; larger matches need larger
maps (about 40x40 tiles per player) and a faster simulation tick. See
[architecture](architecture.md) for the server and [performance](performance.md)
for what is gated.
