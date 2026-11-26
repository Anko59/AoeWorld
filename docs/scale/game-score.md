# Game score

The game score is a small set of deterministic games, played on a real map that
is really in the game by the Rust AI that runs inside the server, and measured
for what the game did and for what it cost. It replaces the synthetic
instruction kernels as the thing that says whether a change is good for the
game. The owner asked for it on 2026-11-26: the performance comment should
measure "arrival, movement work, planning, fights, death numbers, ability to
cross chokes, resource gathered over time, management unit works, amount of
work at different scale scenarios (early game, middle game, late game ...) all
of it on real maps that are really in the game". The name is "game score";
"scoreboard" is the owner-save replay of [scoreboard](scoreboard.md).

Two scenarios exist, `natural` and `late`. Everything here is informational:
nothing gates. The choke and bridge numbers, the Callgrind child and the
pull-request comment come in later changes.

## Run it

```sh
make game-score                                 # both scenarios, 1 worker and this machine's cores
make game-score GAME_SCORE_ARGS='--scenario late --workers 1,4'
make game-score GAME_SCORE_ARGS='--scenario natural --ticks 3000'
make game-score GAME_SCORE_ARGS='--package-directory <dir> --map-hash <64 hex digits>'
```

The package is the owner's Anatolia map: centre 39N 34E, 960 km a side,
compression 240:1, 2,000 tiles, year 600, seed 1, profile `circa600_v1`. It is
generated from the cached geodata sources (`make map-generate`, see
[the 5x and 10x recipe](benches.md)) and never published: one source layer is
CC BY-SA 4.0. The default directory is `.cache/game-score/packages` of the
checkout, because the make target's container sees the checkout only. Any other
package works too, down to a synthetic one.

| flag | meaning | default |
| --- | --- | --- |
| `--scenario` | `natural` or `late`; one flag, one scenario | both |
| `--package-directory` | where `<hash>.json` and its pages are | `.cache/game-score/packages` |
| `--map-hash` | the package to play on | the only package of the directory |
| `--seats` | seats, all played by the Rust AI | 100 |
| `--ticks` | ticks played (ten ticks are one game second); a window that does not fit is left out | the scenario's own |
| `--seed` | seeds the village layout | 1 |
| `--late-percent` | scale of the late-game recipe in percent | the scenario's (100) |
| `--workers` | worker counts, 0 for this machine's cores; one process each | `1,0` |
| `--hash-every` | hash the world every this many ticks | the scenario's (10) |
| `--sample-every` | take the census every this many ticks | the scenario's (50) |
| `--no-orders` | keep no order log: time the run as a live server runs | off |
| `--out` | the JSON report | `.cache/game-score/game-score.json` |

## Scenarios

A scenario is data (`game_score/scenarios/`): how the match starts, how many
ticks are played, which windows of them are reported. The runner plays every
scenario the same way, so a new one (the battle at a choke, the march over a
bridge) is one more constant, plus a `Start` when it needs another beginning.
The whole run is always reported too, as the window `whole`.

| scenario | start | ticks | windows (ticks) | why these |
| --- | --- | --- | --- | --- |
| `natural` | one village per seat, as `POST /match` places them; 100 seats, seed 1 | 10,000 (16 min 40 s) | `early` 1,000 to 3,000, `middle` 8,000 to 10,000 | 200 ticks show nothing: 400 units, no delivery, no fight. From tick 1,000 on, deliveries and orders fill the early window; the first real fights are in the middle one |
| `late` | the owner's late game on every village (`late_game.rs`) at 100 percent: about 9,600 units; 100 seats | 1,200 (2 minutes) | `late` 600 to 1,200 | the first minute is the warm-up: 3,800 villagers walk to work at once and the AI brains take their first decisions. The Steward and the General start working and the armies meet in the second minute |

`late` runs at the full recipe: it starts in 7 s, so the scenario takes under a
minute at one worker (see the measured times below). Nothing is scaled and no
nightly copy is needed.

The Steward, the General and the Vassal do no work in `natural`: in 16 game
minutes no seat gets one (each costs a castle and 1,000 food and 1,000 gold, see
`crates/ai/src/play/management.rs`). `late` shows the Steward and the General.
The Vassal does nothing in either: the AI sends one only where a castle is
threatened.

## The numbers

Each is a whole number per window (`service`), the same at any worker count.

| number | how it is measured |
| --- | --- |
| units alive, buildings standing, villagers, villagers standing idle | the census at the end of the window; idle is a mean of the samples inside |
| units and buildings produced | ids that showed for the first time (ids only grow, so no count of deaths is needed) |
| arrival: orders given, arrived, given up, share arrived, order to arrival (p50, p95 in ticks) | the observer watches every unit's `movement_order` after every tick. A unit within two tiles of the goal has arrived. An order is given up when it is replaced or ends far from its goal, or when its unit is lost |
| order to first step (p50, p95 in ticks) | ticks from an order showing to the unit's position changing. An order the engine answers when given steps in the same tick: 0 |
| movement and planning work | route orders, searches started, answers when given, cache hits, orders stuck, expired and approached (differences of `RouteMetrics`); movers waiting for a route, their wait p95 and the longest wait, at the end and the worst at a sample |
| management units | the Steward's and the General's `StewardWork` and `GeneralWork`, summed over the ticks; orders by issuer (`OrderDemand`) for the Vassal, the Rust AI, the automations and the others. The Vassal keeps no per-tick work counter: its orders are its work |
| fights | hits, attacker-ticks, attack searches, first blows after contact and the ticks they waited (all summed over the ticks), kills and losses of every seat (a loss may be a building) |
| economy | food, wood, gold and stone delivered, every tick, from `recent_deliveries` (below) |

The cost of a window (`cost`) is wall time and memory, so noisy and never
compared between runs: seconds inside the ticks, tick milliseconds (mean, p50,
p95, p99, max), peak resident memory at a census sample, and `instructions`,
`null` until the Callgrind child counts them.

The efficiency rows (`efficiency`) are the cost for each unit of service: per
arrival, per 1,000 resources delivered, per hit, per unit produced. They are
computed from the instructions when the window has them and from the tick
milliseconds otherwise, and `basis` says which. Wall time on a shared machine
moves by tens of percent, so read them as a ratio between two runs of one
machine.

### Why deliveries are read every tick

`recent_deliveries` is a window of twelve buckets of five game seconds: the
bucket of the current tick and the eleven before it. It is not a clean minute.
Read at tick 600 it holds ticks 50 to 600, so reading it every 600 ticks leaves
five seconds out of each minute; read at tick 599 it holds one whole minute, but
only at that tick. The observer reads it after every tick instead. The change
of the window is the tick's own credits minus the bucket that left it when a
bucket ended, and that bucket is a sum of ticks already known, so each tick's
credits come back exactly, with no engine change. A tick where that bookkeeping
contradicts itself is counted in `income_faults`, which stays 0.

## Reading the files

`.cache/game-score/game-score.json` is the report, `"schema": 1`:

```text
schema, name, table                    the table is what the terminal shows
header                                 revision, command, map_hash, cores, load, orders_recorded
scenarios[]
  id, what, note, seats, seed, ticks, hash_every, sample_every
  start                                {kind: "fresh"} or {kind: "late_game", percent}
  measured_at_workers                  the numbers below come from the first run
  windows[]
    id, from_tick, to_tick             ticks from_tick + 1 through to_tick
    service                            whole numbers, the same at any worker count
      size, arrival, planning, management, fights, economy
    cost                               wall time and memory: instructions, tick_seconds, tick_ms, peak_memory_mib
    efficiency                         basis, unit, per_arrival, per_thousand_resources_delivered, per_hit, per_unit_produced
  series[]                             one entry per game minute (600 ticks; the last may be shorter)
    minute, from_tick, to_tick, service, cost
  determinism
    holds, fixed_workers
    runs[]                             workers, native_seconds, setup_seconds, tick_seconds, ticks_per_second,
                                       slowest_tick, peak_memory_mib, orders, final_hash
    against[]                          each run against the first: same_hashes, same_orders, same_numbers, first difference
    diagnosis                          the replay with a hash at every tick, when runs differ
```

A reader compares two reports by `service` exactly and by `cost` and
`efficiency` as ratios. Percentiles are nearest-rank over whole ticks. The
children's own files, with the order log of each run, are in
`.cache/game-score/children/`.

## The table

The terminal table is the one the pull-request comment will show: a block per
scenario, one line per number, one column per window, in plain words, about
thirty lines a scenario. The per-minute line under it shows resources
delivered over the game.

## Determinism

The same match at 1 worker and at N workers must give the same hash at every
hashed tick, the same orders in the same order and the same numbers: that is
the rule of [the working agreement](../../AGENTS.md), and it is the first thing
the game score proves, for every scenario. `make game-score` fails when two
runs differ. It then replays both with a hash at every tick, up to the first
tick that differed, and says in the report which tick that is, whether the unit
and building counts differ there or only the world state, and the first order
that differs. Numbers that differ while the hashes agree are reported and do
not fail the run.

A scenario that is not the same game at every worker count carries
`fixed_workers`: it is played twice at that count and the report says so in its
`note`. Both scenarios are the same game at 1 and 4 workers today.

The engine fixes its thread pool once per process, so each worker count plays
in a child process of its own.

## What it measured on the owner's map

Seed 1, 100 seats, 2026-11-27, the OrbStack machine of the owner's Mac (14
cores), through `make game-score`. The numbers per window are in the pull
request that added the scenarios.

| scenario | 1 worker | 4 workers | of which start |
| --- | --- | --- | --- |
| `natural`, 10,000 ticks | 54 s | 46 s | 5 to 6 s |
| `late`, 1,200 ticks | 55 s | 44 s | 7 s |

These are runs on a quiet machine (load average 2.6 to 4.4). The same scenarios
took 86 s and 107 s (`natural`) and 91 s and 77 s (`late`) on a machine shared
with other builds, so wall time moves by half and more, and at 4 workers a busy
machine can be slower than at 1. The game numbers are the same on a busy and a
quiet machine; they change when the game changes (the route planning of #1122
moved them a little). Both scenarios are inside the two-minute budget of a
pull-request scenario at one worker count.

## What is not here yet

The battle at a choke, the march over a bridge and the choke-crossing counter;
the Callgrind child (instructions per window, memory per unit); the CI job and
the pull-request comment against `dev`; the gate.
