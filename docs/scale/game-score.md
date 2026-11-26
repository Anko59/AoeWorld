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

Today there is one scenario, the early game, and the numbers that the engine
already counts. The other scenarios, the new counters, the Callgrind child and
the pull-request comment come in later changes.

## Run it

```sh
make game-score                                 # the only package of .cache/game-score/packages
make game-score GAME_SCORE_ARGS='--ticks 1000 --workers 1,4'
make game-score GAME_SCORE_ARGS='--package-directory <dir> --map-hash <64 hex digits>'
```

The package is the owner's Anatolia map: centre 39N 34E, 960 km a side,
compression 240:1, 2,000 tiles, year 600, seed 1, profile `circa600_v1`. It is
generated from the cached geodata sources (`make map-generate`, see
[the 5x and 10x recipe](benches.md)) and never published: one source layer is
CC BY-SA 4.0. Any other package works too, down to a synthetic one.

| flag | meaning | default |
| --- | --- | --- |
| `--package-directory` | where `<hash>.json` and its pages are | `.cache/game-score/packages` |
| `--map-hash` | the package to play on | the only package of the directory |
| `--seats` | seats, all played by the Rust AI | 100 |
| `--ticks` | ticks played (ten ticks are one game second) | 600 |
| `--seed` | seeds the village layout | 1 |
| `--workers` | worker counts, 0 for this machine's cores; one process each | `1,0` |
| `--hash-every` | hash the world every this many ticks | 10 |
| `--sample-every` | take the census every this many ticks | 50 |
| `--no-orders` | keep no order log: time the run as a live server runs | off |
| `--out` | the JSON report | `.cache/game-score/game-score.json` |

## What it does

A fresh start: one village per seat, spread over the map as `POST /match`
does it, and every seat played by a Rust AI. Nobody is connected, so no socket
and no clock are involved: the harness starts the match through
`aoe_server::HeadlessMatch` and calls its `tick` in a loop. The server's own
`POST /match` runs the same `launch` and `install` functions, so the two cannot
drift apart.

The engine fixes its thread pool once per process, so each worker count plays
the match in a child process of its own. The report has one column per run.

## The numbers

Each one is named in plain words in the table; the JSON keeps the same names
in `counters`.

- Cost: ticks per second, the slowest tick and which tick it was (a warm-up
  tick shows as a small number), peak memory, seconds to start.
- Size: units alive, buildings standing, villagers, villagers standing idle.
- Movement and planning: movers waiting for a route (at the end and most at
  a sample), the 95th percentile of how long they had waited (ticks), route
  orders given, route searches started, orders answered when given, answers
  taken from the cache.
- Fights: hits landed (summed over the ticks), kills and losses of all seats.
- Economy: resources delivered in the last game minute. The engine keeps no
  longer window, so a run longer than 600 ticks shows its last minute only.

## Determinism

The same match at 1 worker and at N workers must give the same hash at every
hashed tick and the same orders in the same order: that is the rule of
[the working agreement](../../AGENTS.md), and it is the first thing the game
score proves. `make game-score` fails when two runs differ. It then replays
both with a hash at every tick, up to the first tick that differed, and says in
the report which tick that is, whether the unit and building counts differ
there or only the world state, and the first order that differs. A counter that
differs while the hashes agree is reported and does not fail the run.

## Reading the files

`.cache/game-score/game-score.json` is the report: `header` (revision, map,
flags), `determinism` (the verdict and the diagnosis), `runs` (the counters,
the census samples, a digest per hashed tick) and `table` (what the terminal
shows). The children's own files, with the order log of each run, are in
`.cache/game-score/children/`.
