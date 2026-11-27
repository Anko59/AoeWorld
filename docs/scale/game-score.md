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

Four scenarios exist. `natural` and `late` are played by the Rust AI; `march` and
`battle` are staged at a real choke of the map, with no AI seat
([the choke scenarios](game-score-chokes.md)). Everything here is informational:
nothing gates. The Callgrind child comes in a later change. `make ship` writes the
game score into the pull request ([below](#in-make-ship)).

## Run it

```sh
make game-score                                 # the four scenarios, 1 worker and this machine's cores
make game-score GAME_SCORE_ARGS='--scenario late --workers 1,4'
make game-score GAME_SCORE_ARGS='--scenario natural --ticks 3000'
make game-score GAME_SCORE_ARGS='--package-directory <dir> --map-hash <64 hex digits>'
make game-score-pr                              # this branch against its merge base, as make ship runs it
```

The package is the owner's Anatolia map: centre 39N 34E, 960 km a side,
compression 240:1, 2,000 tiles, year 600, seed 1, profile `circa600_v1`. It is
generated from the cached geodata sources (`make game-map`, see
[the geodata workflow](../geodata/workflow.md#the-owners-pinned-map-make-game-map))
and never published: one source layer is CC BY-SA 4.0. The default directory is
`packages` in the [shared home](#the-shared-home), which every worktree and every
container sees. Any other package works too, down to a synthetic one.

| flag | meaning | default |
| --- | --- | --- |
| `--scenario` | `natural`, `late`, `march` or `battle`; one flag, one scenario | every scenario of the map |
| `--package-directory` | where `<hash>.json` and its pages are | `packages` of the shared home |
| `--map-hash` | the package to play on | the only package of the directory |
| `--seats` | seats, all played by the Rust AI (a staged scenario has the seats of its armies) | 100 |
| `--ticks` | ticks played (ten ticks are one game second); a window that does not fit is left out | the scenario's own |
| `--seed` | seeds the village layout | 1 |
| `--late-percent` | scale of the late-game recipe in percent | the scenario's (100) |
| `--workers` | worker counts, 0 for this machine's cores; one process each | `1,0` |
| `--hash-every` | hash the world every this many ticks | the scenario's (10) |
| `--sample-every` | take the census every this many ticks | the scenario's (50) |
| `--no-orders` | keep no order log: time the run as a live server runs | off |
| `--parallel` | play every run at once, one process each (all scenarios at all worker counts): the pull request's way, about the time of the longest run | off |
| `--out` | the JSON report | `.cache/game-score/game-score.json` |
| `--survey-chokes`, `--survey-at x,y` | survey the map for chokes instead of playing it (written to `chokes.txt` beside `--out`), and measure the neck at a tile in full | off |

## Scenarios

A scenario is data (`game_score/scenarios/`): how the match starts, how many
ticks are played, which windows of them are reported. The runner plays every
scenario the same way, so a new one is one more constant, plus a `Start` when it
needs another beginning.
The whole run is always reported too, as the window `whole`.

| scenario | start | ticks | windows (ticks) | why these |
| --- | --- | --- | --- | --- |
| `natural` | one village per seat, as `POST /match` places them; 100 seats, seed 1 | 10,000 (16 min 40 s) | `early` 1,000 to 3,000, `middle` 8,000 to 10,000 | 200 ticks show nothing: 400 units, no delivery, no fight. From tick 1,000 on, deliveries and orders fill the early window; the first real fights are in the middle one |
| `late` | the owner's late game on every village (`late_game.rs`) at 100 percent: about 9,600 units; 100 seats | 1,200 (2 minutes) | `late` 600 to 1,200 | the first minute is the warm-up: 3,800 villagers walk to work at once and the AI brains take their first decisions. The Steward and the General start working and the armies meet in the second minute |
| `march` | 800 Militia on one bank of a real choke (a river ford), one formation move to the other | 2,400 (4 minutes) | `crossing` | the ability to cross a choke, with nothing else going on ([chokes](game-score-chokes.md)) |
| `battle` | two armies of 400 (melee and ranged) sent across the same choke from opposite banks, attack-moving | 4,500 (7.5 minutes) | `fight` | a fight that always happens, in the narrows |

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
| choke, battle | in `march` and `battle` only: units across the line of the choke, the neck's throughput, standing still in the neck, arrival at the goal; the first blow, who fought and who only queued, who survived ([chokes](game-score-chokes.md#the-numbers)) |

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
      choke, battle                    only in a staged scenario (march, battle)
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
`note`. All four scenarios are the same game at 1 and 4 workers today.

The engine fixes its thread pool once per process, so each worker count plays
in a child process of its own.

## The shared home

One folder serves every worktree of a checkout and every container: `aoe-game-score`
in the common Git directory, beside `aoe-cargo`. The Makefile already mounts that
directory in every container (it is where `aoe-cargo` lives), and Git never tracks it.
Under `make` this is the only home: the containers mount the checkout and the common Git
directory and nothing else, and the goals do not pass `AOE_GAME_SCORE_HOME` to them. That
variable names another home only for the harness binary run directly (the tests, a machine
whose common directory is on a slow disk).

```text
aoe-game-score/
  packages/            the map package: `make game-map` writes it, `make game-score` reads it
  results/<key>.json   the results cache, one entry per game state (see below)
  base/                the scratch worktree the merge base is built and played in
  base.lock/           present while a ship uses base/ (see "The merge base" below)
```

`make game-map` and `make game-score` use `packages` by default; `--package-directory`
and `AOE_MAP_PACKAGE` still name another place. In CI (`GITHUB_ACTIONS` set) the default
stays `.cache/game-map/packages`, where the `game-map` workflow and its restore action
keep the package. The geodata sources (7 GB) stay in `.cache/geodata` of the checkout.

## In `make ship`

The owner, 2026-11-27: "The performance comment should measure things like arrival,
movement work, planning, fights, death numbers, ability to cross chokes, resource
gathered over time, management unit works, amount of work at different scale scenarios
[...] I want my coding agents to stop fighting the CI and, instead have it work on their
side and really show if work is going in the right direction."

The real map cannot be generated on GitHub's runners (one data provider refuses them),
so the game score does not run in CI. It runs on the machine where the agent works, as
part of `make ship`, and is written into the pull request. The game is deterministic,
so the `service` numbers do not depend on the machine. It is informational: it never
fails or blocks a ship, there is no threshold and no baseline file.

```sh
make game-score-pr          # what ship runs, by hand: decide, play, compare, print
make ci-explain             # says "game-score (make ship, not a CI job): runs because ..." or "skipped because ..."
```

1. **When it runs.** Only when the change can affect the game: a changed path in the build
   closure of `aoe-server` (simulation, Rust AI, map, protocol, SDK, core, scenario), or one
   of the inputs `local_checks` of [`gates/ci-rules.json`](../../gates/ci-rules.json) names
   ([CI selection](../ci-selection.md#local-checks-the-game-score)). Otherwise ship prints one
   line (`skipped because ...`) and does nothing else. If the map is not in the shared home
   (`make game-map`), ship says so in one line and goes on.
2. **The branch.** Every scenario of the set (the four on the owner's map) at 1 and 4 workers (the determinism check: two different
   counts, not every core, because several agents ship at once on one machine; one constant,
   `WORKERS` in `crates/harness/src/game_score/pr/real.rs`), every run in a process of its own
   and all at once (`--parallel`): 75 to 140 s on the owner's Mac, where the other agents'
   builds were running (`march` and `battle` take 1 to 6 s each, so the set costs what `natural`
   costs). The release build of the checkout comes first (39 to 67 s once the dependencies are
   compiled).
3. **The merge base, without paying for it every time.** The `service` numbers are a function
   of the game's code, the map and the scenarios, so a result is stored under a hash of
   exactly those, not under a commit: every file (path and Git object id) of every crate in the
   build closure of the server and of `crates/harness/src/game_score` (scenarios, counters,
   report schema), `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` and
   `crates/harness/Cargo.toml`, the map hash and the report schema, less the files the `ignore`
   globs of the rule name (tests, benches, examples and `.md` files of a crate): a change to
   those does not run the check, so it cannot change the key either. Two commits with the same
   game code share one entry. A run of a branch stores its own entry (unless the working tree
   differs from HEAD in a file that counts, or the runs disagreed); the merge base is played only
   when no entry has its key. On that miss the persistent scratch worktree `base/` moves to the
   merge base (a detached checkout), builds it incrementally and plays it at one worker, both
   scenarios at once, then stores it. One ship at a time uses `base/`: it holds the lock
   `base.lock/` (a directory made atomically, with a heartbeat file a live holder touches every
   5 s; a lock whose heartbeat is 60 s old belongs to a dead process and is taken over). A second
   ship waits up to 15 min, looks in the cache again when it gets the lock (the first one stored
   the entry it was after, most of the time) and plays the merge base only if it is still
   missing. If it waits longer, or the merge base cannot be built, ship says so in one line and
   shows the branch's numbers alone.
4. **The comparison.** A table per scenario: number, merge base, this branch, change.
   `service` numbers are compared exactly. A number that changed is a row (those the table
   names first, the rest collapsed under them); the others are a count ("93 numbers
   unchanged"). Wall time and memory sit beside, in a collapsed block labelled noisy, and are
   never judged. A cost per unit of service (`efficiency`) is judged only when both sides
   counted instructions (`cost.instructions`), which no run does yet. A scenario whose
   definition differs between the two sides (ticks, seats, seed, windows) is not compared, and
   says why.
5. **The verdict.** The section's first lines say which case it is.
   - **Same game**: every `service` number is the merge base's. Nothing is judged (a cost per
     unit of service on instruction counts will be, exactly, once a run has them).
   - **Different game: N numbers moved, M beyond their usual spread.** A deterministic game is
     still chaotic: any change of behaviour perturbs every number a little, and one seed is
     one sample. So a judged number (share of orders arrived, order to arrival and to first
     step p95, movers waiting, wait for a route, route orders stuck or expired, idle
     villagers, resources delivered; in the staged scenarios units across the line, the crossing
     rate, standing still in the neck, units waiting, first blow, units that only queued)
     gets a line only when it moved beyond what it does across
     seeds on the same code ([the usual spread](#the-usual-spread)): by more than twice its
     measured spread, never under 5 percent, and by more than a minimum absolute move (the
     range it showed across the seeds, and for a small count 10, for a delay 3 ticks), so that
     a count of 2 that becomes 7 is not a finding. Every other move is in the table only.
     Better moves are held to the same tolerance.
   - Determinism (the same at 1 and 4 workers) and crash lines follow either way.

### What it costs

Measured on 2026-11-27 on the OrbStack machine of the owner's Mac (14 cores, other agents'
builds and tests running: load average 4 to 5), through `make game-score-pr` at 1 and 4
workers, in seconds:

| case | merge base | this branch | the whole command |
| --- | --- | --- | --- |
| a hit for the merge base, a new game state for the branch: the usual re-ship after a change to the game | lookup 0.0 | build 48, play 78 | 131 |
| a miss for the merge base (its game state was never played), scratch worktree already there | build 88, play 67 | build 49, play 110 | 349 |
| both hit: the same game state shipped again (a fix of a review finding, a rebase onto a dev that did not touch the game) | lookup 0.0 | lookup 0.0 | 7 (container, `cargo metadata`, Git) |
| the change cannot affect the game, or there is no map | not run | not run | 2 to 8 (one line) |

Without a miss, ship's gates (about 300 s) gain at most about 2 minutes, and the game score
runs beside the review, which takes longer: a ship that waits for its review often gains nothing
at all (the section says how much it did add). A miss costs about 3.5 more minutes: the merge
base's release build (88 s here, 125 s on a worktree whose `target/` was empty, the third-party
crates coming from the shared compile cache; builds recompile only the crates the merge base
changed) and its two scenarios (67 s, one worker, at once). A miss for the merge base is the usual
case for the first ship of a branch cut from a `dev` that has game changes nobody played yet;
every later ship of that branch hits. The scratch worktree takes 0.8 GB, the results 100 KiB
each (400 kept).

### Explaining a move that was intended

Nothing gates, so a move beyond its usual spread is not a failure to clear. When it was meant,
one line of the pull request description says why, anywhere in it, one line per number:

```text
game-score: late arrived -3% because the AI now waits for its escort before it marches
game-score: natural/early first-step-p95 +20% because orders go through the new planner queue
```

`<scenario>` or `<scenario>/<window>`, the number's name, the change in percent against the
merge base, `because`, a reason. Without a window the line covers every named window of the
scenario (`whole` is shown but not judged). The names: `arrived`, `arrival-p95`,
`first-step-p95`, `waiting`, `wait-p95`, `stuck`, `expired`, `idle`, `delivered`,
`crossed`, `crossing-rate`, `first-crossing`, `last-crossing`, `waiting-near-side`,
`still-inside`, `at-goal`, `first-blow`, `queued`,
`cost-per-arrival`, `cost-per-resources`, `cost-per-hit`, `cost-per-unit`. A move with such a
line is shown as explained, with the reason; one without says "moved beyond its usual spread
(one seed)" and the line it would take. A line that understates the move says so, one that
matches no move beyond its spread is listed, and so is one whose name is unknown.

### What ship writes

- The terminal prints the same table when the game score ends, before the pull request is
  made ready. The child runs beside the review. Once the review has ended (passed or failed):
  a check that never got a long-job slot (every one still busy) is stopped at once, one line
  says so; a check that is running gets 10 more minutes, then is stopped, one line says so;
  one that has ended is read. The 40-minute limit of the whole check stays. An informational
  number never holds a ship.
- One comment on the pull request, the first line `<!-- aoe-game-score -->`, edited in place
  on a re-ship: the case and the verdict, the tables, what each side cost and what the whole
  check added to this ship beyond the review's own time (0 when it ended before the review
  did), and the whole report in a collapsed JSON block that `make ci-report` can read later
  (both sides' windows; the merge base's side is dropped, then the block, when GitHub's
  65,536 characters would be passed).
- The commit status `harness/game-score` on the pushed commit: always `success` by design
  (GitHub has no neutral state for a status, and nothing here gates), the summary in 140
  characters, a link to the comment. A skipped run posts nothing. When dev is merged into the
  pull request later, the new head has no status: the comment still says what was measured.

### The usual spread

How much a judged number moves when the game has not changed: the same code played at seeds 1
to 8 (one worker, every scenario of the set), `dev` at the commit named in
[`scenarios/spread.json`](../../crates/harness/src/game_score/scenarios/spread.json), which the
verdict reads. It sits beside the scenarios because it is a property of them: a test fails
when a scenario, a window or a judged number has no entry, so changing a scenario means
measuring again, and the file is part of the results cache key. The measure is robust, the
median absolute deviation over the median (MAD / median, percent), with the range across the
seeds beside it; the tolerance column is what the verdict applies. A number nobody measured is
judged at 25 percent.

```sh
for n in 1 2 3 4 5 6 7 8; do
  make game-score GAME_SCORE_ARGS="--seed $n --workers 1 --parallel --out <directory>/seed-$n.json"
done
make game-score-spread GAME_SCORE_SPREAD_ARGS="--reports <directory> --commit <sha> --date <YYYY-MM-DD>"
```

Measured on 2026-11-27 at `5c5222ff` (a branch of `dev` whose `natural` and `late` are `dev`'s:
the same numbers as at `a2b432f2`), seeds 1 to 8, one worker. The 100-seat scenarios of this page
are below; `march` and `battle` are in [the choke scenarios](game-score-chokes.md#the-usual-spread):

| scenario | window | number | median | spread (MAD / median) | range | tolerance |
|---|---|---|---|---|---|---|
| late | late | share of orders arrived | 66.4% | 1.9% | 62.1% to 69.3% | 5% and 7.2% |
| late | late | order to arrival, p95 (ticks) | 614.5 | 4.2% | 363 to 644 | 8% and 281 |
| late | late | order to first step, p95 (ticks) | 5.5 | 63.6% | 1 to 16 | 127% and 15 |
| late | late | resources delivered | 23,245 | 2.4% | 20,812 to 23,914 | 5% and 3,102 |
| late | late | route orders expired | 0.5 | 100.0% | 0 to 7 | 200% and 10 |
| late | late | route orders stuck | 6,384 | 18.3% | 5,175 to 9,117 | 37% and 3,942 |
| late | late | wait for a route, p95 at the worst sample (ticks) | 1,200 | 0.0% | 293 to 1,200 | 5% and 907 |
| late | late | movers waiting for a route, peak | 51 | 60.8% | 10 to 215 | 122% and 205 |
| late | late | villagers standing idle | 2.9% | 10.3% | 2.1% to 4.3% | 21% and 2.2% |
| natural | early | share of orders arrived | 97.0% | 0.3% | 94.0% to 97.5% | 5% and 3.5% |
| natural | early | order to arrival, p95 (ticks) | 59.5 | 4.2% | 56 to 63 | 8% and 7 |
| natural | early | order to first step, p95 (ticks) | 1 | 0.0% | 1 to 1 | 5% and 3 |
| natural | early | resources delivered | 43,792 | 0.6% | 42,797 to 44,062 | 5% and 1,265 |
| natural | early | route orders expired | 0 | 0.0% | 0 to 0 | 5% and 10 |
| natural | early | route orders stuck | 4,852 | 34.0% | 608 to 9,330 | 68% and 8,722 |
| natural | early | wait for a route, p95 at the worst sample (ticks) | 321.5 | 85.2% | 0 to 598 | 170% and 598 |
| natural | early | movers waiting for a route, peak | 2 | 25.0% | 0 to 3 | 50% and 10 |
| natural | early | villagers standing idle | 0.1% | 0.0% | 0.0% to 0.3% | 5% and 1.0% |
| natural | middle | share of orders arrived | 91.7% | 0.4% | 89.9% to 92.5% | 5% and 2.6% |
| natural | middle | order to arrival, p95 (ticks) | 178.5 | 1.1% | 166 to 185 | 5% and 19 |
| natural | middle | order to first step, p95 (ticks) | 1 | 0.0% | 1 to 1 | 5% and 3 |
| natural | middle | resources delivered | 110,680 | 0.7% | 108,439 to 112,773 | 5% and 4,334 |
| natural | middle | route orders expired | 0 | 0.0% | 0 to 5 | 5% and 10 |
| natural | middle | route orders stuck | 13,863 | 15.7% | 10,224 to 24,786 | 31% and 14,562 |
| natural | middle | wait for a route, p95 at the worst sample (ticks) | 105 | 11.0% | 91 to 599 | 22% and 508 |
| natural | middle | movers waiting for a route, peak | 4.5 | 33.3% | 2 to 8 | 67% and 10 |
| natural | middle | villagers standing idle | 2.6% | 9.4% | 2.1% to 3.0% | 19% and 1.0% |

What it says. Stable across seeds, so a move of them is worth a line: the share of orders
that arrived (spread 0.3 to 1.9 percent), the order to arrival p95 (1 to 4 percent) and the
resources delivered (0.6 to 2.4 percent); the share of idle villagers is small enough that a
move under a percentage point is nothing, and in the `early` window it never leaves 0.3
percent. Not stable at all, so a move of them says little on one seed: the route orders stuck
(16 to 34 percent, from 608 to 9,330 in `early`), the movers waiting for a route at the worst
sample (25 to 61 percent) and the p95 of their wait (85 percent in `early`; in `late` it sits at
the cap of 1,200 ticks on five seeds and is 293 to 566 on three), the route orders expired
(0 on most seeds, up to 5 in `middle` and 7 in `late`), and in `late` the order to first step
p95 (1 to 16 ticks). Their tolerance is wide on purpose: the verdict looks at them only for a
move no seed showed.


## What it measured on the owner's map

Seed 1, 100 seats, 2026-11-27, the OrbStack machine of the owner's Mac (14
cores), through `make game-score`. The numbers per window are in the pull
request that added the scenarios.

| scenario | 1 worker | 4 workers | of which start |
| --- | --- | --- | --- |
| `natural`, 10,000 ticks | 54 s | 46 s | 5 to 6 s |
| `late`, 1,200 ticks | 55 s | 44 s | 7 s |
| `march`, 2,400 ticks | 1.3 s | 1.4 s | 0.5 s |
| `battle`, 4,500 ticks | 5.6 s | 5.7 s | 0.8 s and 0.4 s |

These are runs on a quiet machine (load average 2.6 to 4.4). The same scenarios
took 86 s and 107 s (`natural`) and 91 s and 77 s (`late`) on a machine shared
with other builds, so wall time moves by half and more, and at 4 workers a busy
machine can be slower than at 1. The game numbers are the same on a busy and a
quiet machine; they change when the game changes (the route planning of #1122
moved them a little). Every scenario is inside the two-minute budget of a
pull-request scenario at one worker count; the two staged ones need no world of 100 villages
and cost seconds ([the choke scenarios](game-score-chokes.md)).

## What is not here yet

The Callgrind child (instructions per window, memory per unit), which makes the
cost per unit of service judgeable; a way for the map to reach CI (the owner decides
it); the gate.
