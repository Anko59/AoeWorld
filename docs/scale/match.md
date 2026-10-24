# The real game at scale: 1 human, 99 AIs

`make scale-match` is the end-to-end proof harness for the promised scale: one
human in the browser, 99 Python AI opponents, a France map at 1:30 (16,000 x 16,000
two-metre tiles), about 100,000 units, and the server holding 20 Hz (50 ms per
tick) for a long session. It runs the real `aoe-server` release binary, so it
re-runs as the simulation and fan-out optimisations land; the headless
component benchmark (`make perf-scale`, [measurements](measurements.md)) stays
the place to find hot spots.

## What a run does

1. Builds and starts a release `aoe-server` (loopback port 9100, a private
   admin secret, `target/scale-match/server.log`).
2. `POST /match` with 100 slots (slot 0 human, 1-99 `ai`), a France map, the
   population cap and optional start units, timing the creation.
3. Builds the release wheel (`sdk-python-steps`, in the same heavy slot) and starts the bots in
   `--hosts` sibling `aoeworld/sdk-python` containers, each one Python process
   running `python -m aoe_ai host` (many bots on one native runtime and one
   thread).
4. For `--minutes` of wall time, once a second reads the server's own per-tick
   records (`GET /match/stats`: whole tick, `advance`, start lateness, orders
   applied, units alive), and every 5 s the server's CPU and RSS (`/proc`) and
   the bot containers' CPU and memory (`docker stats`).
5. With `--browser`, after `--browser-delay` seconds starts Chromium 1.64
   (software WebGPU, the real art pack) in a sibling container, opens the game
   as the human (a plain `Hello` claims slot 0) and records
   `requestAnimationFrame` intervals: first around the human's village, then
   zoomed out over it, for `--browser-minutes` in total.
6. Writes `reports/scale/match/match-<label>-<revision>.json` and rewrites the
   "latest run" block below.

```sh
make scale-match                         # defaults: 100 slots, 16,000 tiles, cap 1,000, 20 min
SCALE_MATCH_ARGS="--minutes 30 --browser --start-villagers 800 --start-militia 100" make scale-match
make scale-match-smoke                   # 4 players, 1 minute, flat 500-tile map
```

Options (`aoe-harness scale-match --help`): `--players --humans --tiles
--population-cap --start-villagers --start-militia --minutes --hosts --map
procedural|package|flat --map-hash --package-directory --port --seed
--asset-pack --browser --browser-minutes --browser-delay --out --summary
--no-summary --label`. The browser step needs `make build-wasm` (the server serves `web/`) and
an original-art pack (`AOE_ASSET_PACK=local-assets/packs/<hash>`).

## The match rules behind it

`POST /match` accepts (see [the AI SDK](../ai-sdk.md)):

* `population_cap` (5 to 5,000, default 200, the AoE rule): the most population
  any player reaches however many houses stand. World capacities are sized
  from the seats and this cap ([capacities](capacity.md)): 1,024 buildings per
  seat (more above cap 200) and one worker per unit of population.
  100 players x 1,000 = 100,000 units.
* `start_units: {"villagers": n, "militia": m}`: the scenario hook. After the
  standard start kit (Town Center, villagers, scout) every player gets `n`
  extra villagers and `m` militia placed on free ground around the village,
  without cost. They count toward population: a seeded player above its
  ceiling trains nothing until it builds housing.

The report states how many units come from where: `setup.seeded_units` (the
hook), `setup.units_at_first_tick` (kit plus seeding) and
`run.units_gained_by_play` (what the bots trained). Bots grow slowly (a young
economy has a few dozen units after minutes), so a run at 100,000 units needs
the hook; a run without it measures real bot play at far lower counts.

## Maps

`--map procedural` (default) sends a France-centred request (46.6 N, 2.5 E,
960 km square at 30:1 = 16,000 tiles) with no source layers: procedural
terrain, the same world as `make perf-scale`. `--map package --map-hash H`
opens a package the server holds in `--package-directory`; a source-backed
France overview package is made with the usual flow:

```sh
AOE_GEODATA_CACHE=<cache> AOE_MAP_REQUEST=local-assets/requests/france-scale-16000.json \
  AOE_MAP_PACKAGE=local-assets/map-packages make map-generate
```

(the request is `requested_side_meters: 960000`, compression 30:1,
centre 46.6 N 2.5 E). Start sites are spread over the whole map on a grid with
a search for connected land near each ideal point, and each gets the full
start kit (`initialize_village`). A village whose kit could not be placed in
full is counted: `setup.partial_starting_kits` in the report (and its summary
row) must be 0; the match response carries the same counter.

## Reading the numbers

* **tick total** is the whole server tick: applying orders, `advance`, and
  building every session's view (fan-out). `advance` is the simulation alone;
  `orders, views and fan-out` is the difference. A **missed tick** started over
  50 ms late or took over 50 ms. The server skips missed intervals rather than
  catching up, so a slow server runs the game slower than real time: `game
  speed` is ticks per second / 20 (1.0 holds real time).
* Server CPU is the process; the bots' CPU and memory are the containers'.
  Everything shares the development machine (12 cores) with whatever else is
  running; the report records the machine's core count only, so note the load
  when comparing runs.
* Browser frames are software WebGPU in a container (SwiftShader), not a GPU:
  they bound what the client does on CPU, not what a player's graphics card
  does. They are `requestAnimationFrame` intervals of the real game page.
* A synthetic workload at this scale does not demonstrate finished RTS
  performance or dedicated hardware qualification.

## World capacity run

`world-capacity` (2026-10-22, branch `fix/world-capacity`: commit d8dddfd0's code, built from the
working tree before that commit, on `play/integration` 70c5da80). The owner's setup: the
source-backed France package (`cf552006...`, 16,000 tiles), 100 slots (slot 0 human and empty,
99 `aoe_ai` bots on 8 hosts), population cap 200, standard starts, no seeding, 25 minutes of wall
time; machine load 10 to 13 from other agents.

```sh
SCALE_MATCH_ARGS="--players 100 --humans 1 --population-cap 200 --minutes 25 --hosts 8 \
  --map package --map-hash cf55200686bf3117e7ac33d48c3b8ae0354c8709cb33d54ba070def4fed12910 \
  --package-directory local-assets/map-packages --label world-capacity --no-summary" make scale-match
```

| Ticks | Game time | Tick p50 / p95 / p99 (ms) | `advance` p50 / p95 | Missed | Units at end | Bot buildings | Feudal |
|---|---|---|---|---|---|---|---|
| 29,393 (19.6 a second) | 24.5 min (0.98x) | 25.4 / 51.3 / 87.4 | 20.9 / 38.3 | 1,823 (6.2%) | 4,249 | 1,591 | 77 of 99 |

The bots sent 1,692 build orders and completed 1,473 buildings; 77 of them reached Feudal Age
and kept building. Build refusals: 155 `REJECTED_INVALID_PLACEMENT`, 36 `REJECTED_UNREACHABLE`; no
`REJECTED_QUEUE_FULL` and no `REJECTED_WORLD_LIMIT` on any order. By a lower bound (99 Town
Centers of 16 tiles, at most 594 farms, at least 898 other buildings of 4 tiles or more) the
buildings cover over 5,190 collision tiles, past the old overlay's 4,096 that stopped the
owner's game. 0 bot sessions dropped, 0 errors. Server 0.75 cores mean, 921 MiB RSS. The bots
reached about 43 population each, far below the cap: the 100-village Feudal world of 13,600
buildings is covered by the simulation test in [capacities](capacity.md), not by this run.
50,250 of the 54,921 refusals are the bots' own `move` orders to invalid destinations.

## Results on this branch

The runs up to `france-real-seeded-reviewed` below are **superseded** by `france-real-seeded-fanout`
(see "Latest run" at the end): they ran before dev's fan-out rebuild (#556) and before every village got a
starting kit (#553), so they are history, not the current state. The runs that report a kit ran with a 512-node
starting limit, so only about 16 of the 100 villages had a full kit; that limit is removed and those runs'
`advance` times understate a full-kit world.

Measured 2026-10-22 at revision 99526a24 plus the commit that follows it (far-land start fallback, container stats fix) on the 12-core development machine while other agents ran
gates and benchmarks (1-minute load average 13 to 15 throughout), so every number
is pessimistic by an unknown amount. The harness container had 6 CPUs (one heavy
slot); the bot containers had none capped. Reports: `reports/scale/match/match-*.json`.

| Run | What | Tick p50 / p95 (ms) | Missed | Units | Game speed |
|---|---|---|---|---|---|
| `france-real-seeded` | source-backed France package (real terrain, overview package `cf552006...`), 100 slots, cap 1,000, 900 seeded units per player, 8 bot hosts, 9 min | 718 / 1,021 (max 9,665) | 751 of 751 | 90,400 (90,000 seeded, none gained by play) | 0.07 |
| `france-real-seeded-after-fix` | the same run after rebasing onto dev with the movement 11x change (#552) and the session fix below, 10 min | 221 / 338 (max 15,988) | 2,248 of 2,259 (99.5%) | 90,291 (90,000 seeded) | 0.19 |
| `france-real-seeded-final` | rebased again onto dev with the wildlife and fog-memory change (#554), 10 min, 1-minute load 5 to 14 | 189 / 277 (max 16,777) | 2,771 of 2,808 (98.7%) | 90,260 (90,000 seeded) | 0.23 |
| `france-real-seeded-reviewed` | final code after the review fixes (acks never dropped, rejoin keeps commands, counters), 10 min, load 10 to 17 | 197 / 272 (max 16,240) | 2,727 of 2,732 (99.8%) | 90,260 (90,000 seeded) | 0.23 |
| `france-real-seeded-fanout` | rebased onto dev with the shared-frame fan-out (#556), the arena AI (#553) and the session policy; load 11 to 20 | 120 / 222 (max 12,477) | 4,132 of 4,174 (99%) | 90,235 (90,000 seeded) | 0.35 |
| `procedural-bots-only` | procedural France-centred 16,000 tiles, 100 slots, cap 1,000, no seeding, 4 bot hosts, 8 min | 27.1 / 39.8 (max 789) | 40 of 9,702 (0.4%) | 400 to 614 | 1.01 |
| `browser-baseline` | 2 slots on a flat 500-tile map, idle economy, same browser | 0.13 / 0.41 | 0 | small | 1.0 |

* **Full scale does not hold real time.** With 90,400 units the server ran at
  0.07 times real time (the `advance` alone is 687 ms p50 against a 50 ms budget,
  consistent with the headless 381 ms of `make perf-scale` plus the cost of real
  bot orders and a source-backed terrain). Fan-out and order handling added only 30
  ms p50 at that load. Server CPU averaged 1.4 cores, 596 MiB RSS: the tick is one thread.
**After the rebase and the session fix (`france-real-seeded-after-fix`, revision 7a391d27 on dev 8bb27ec5, 1-minute load 4 to 11, 6-CPU slot).**
Same map, same 90,000 seeded units, 99 bots over 8 hosts, 10 minutes. No bot session ended for good
(0 of 99 closed, 0 errors). That run predates the rejoin counters, so how many sessions were dropped
and rejoined by the SDK is unknown for it. Tick p50 221 ms, p95 338 ms, p99 387 ms; the simulation
step (`advance`) is 102 ms p50 / 140 ms p95 (was 687 / 957) and orders, per-session views and fan-out
are 122 ms p50 / 211 ms p95 (was 30 / 59: the bots now live long enough to send about 1,400 orders a
second, 93% of them rejected `REJECTED_QUEUE_FULL` because the server applies one tick's orders per tick). Game speed 0.19 times
real time against 0.07 before; the 50 ms budget is still missed by 4 to 5 times. Server CPU 2.2 cores mean
(8.9 max: parallel movement), 932 MiB RSS; bots 0.3 cores mean, 202 MiB. Match creation 97 s with the
seeding. Browser (software WebGPU, same machine, the server at 5 ticks a second): 2.8 fps around the
village and 2.7 fps zoomed out, frame p50 350 ms: still far from 30 fps. One session was dropped by the server
for a failed socket write: the browser container ending at the end of its measurement.

**Final run (`france-real-seeded-final`, revision f89aee4a on dev 0c517c4c).** Tick p50 189 ms, p95 277 ms,
p99 331 ms; `advance` 52 / 82 ms (the simulation alone is now near the 50 ms budget); orders, views and
fan-out 127 / 221 ms, now the larger part (1,606 orders a second applied, 93% rejected `REJECTED_QUEUE_FULL`).
Game speed 0.23x. 0 bot sessions permanently lost, 0 errors (rejoin counters did not exist yet). Server 2.3 cores mean (9.1 max), 950 MiB RSS;
bots 0.4 cores mean, 201 MiB. Match creation 97 s. Browser 2.6 fps around the village, 2.6 fps zoomed
out (software WebGPU). The next target is the per-session fan-out and the order path, not movement.

**Reviewed run (`france-real-seeded-reviewed`, revision fdddd7d3 on dev 0c517c4c; 1-minute load 10 to 17).**
Tick p50 197 ms, p95 272 ms, p99 311 ms; `advance` 35 / 54 ms; orders, views and fan-out 159 / 225 ms
(1,723 orders a second applied, 98% rejected, mostly `REJECTED_QUEUE_FULL`). Game speed 0.23x, 2,727 of
2,732 ticks missed. **Sessions: 0 dropped and rejoined (0 drops, 0 rejoins, 0.0 s away), 0 permanently lost**, 0 errors,
and the server logged no dropped session. The bots connected 3.0 s after their hosts started and the measured
window starts after that (match creation 104 s). Server 2.3 cores mean (8.9 max), 938 MiB; bots 0.4 cores.
Browser: 2.6 fps around the village, 2.4 zoomed out (software WebGPU).

**On dev's fan-out (`france-real-seeded-fanout`, revision 0c504af0 plus the placement and host-retry fixes below; 1-minute load 11 to 20).**
Tick p50 120 ms, p95 222 ms, p99 316 ms; `advance` 107 / 178 ms; orders, views and fan-out
**13.5 / 49.9 ms** (was 159 / 225: #556 removed the fan-out as the dominant cost). Game speed 0.35x
(was 0.23x), 4,132 of 4,174 ticks missed. **Sessions: 0 dropped and rejoined, 0 permanently lost**, 0 errors,
no dropped-session warning. The simulation step is now the whole gap: `advance` is 107 ms against the 50 ms budget
(heavier than before because every village now gets a full starting kit, #553). Server 2.0 cores mean, 986 MiB;
bots 1.0 core. Browser (software WebGPU, machine heavily loaded): 1.5 fps around the village, 1.8 zoomed out.
Match creation 208 s (the full kit and the wider land search); bots connected 16 s after their hosts started, after the
host learned to retry a join that times out against a busy server. Two fixes were needed to start 100 villages on
this map with #553: placement could leave a village half made (the kit failed after the Town Center stood, the next
site then met `AlreadyStarted`), so a kit that cannot be placed in full no longer fails the start; and the far-land
fallback spreads its candidates instead of taking the nearest coast.

**The session loss.** The server used to end a session whenever its outgoing queue (64 messages) was
full at one of its sends, and to give up on a socket write after 500 ms. Which of the two reset the 84 bots
was not measured (the old run had no logging); a stalled tick is a plausible trigger for both. The policy is
now: a full queue skips that message and the next tick's delta (computed against what the session last
received) carries it; only a closed queue ends a session; the first views of a subscription wait for room
in a task; socket writes get 20 s and log a warning when they fail; and the Rust SDK client rejoins its
slot with its token when a connection is lost, taking a fresh snapshot. Tests: several sessions survive a
stalled reader and catch up (`crates/server/tests/slots.rs`), the SDK rejoins after another connection
takes its slot (`crates/sdk/tests/local_server.rs`). With the fix, no session ended for good at 0.2 s ticks;
it is not proven that the old cause was the queue. The 84 resets and the later zeros are different measures (the
old SDK had no rejoin; the new one rejoins silently), so they are not a before/after; the `france-real-seeded-reviewed` run (superseded, above) and
`france-real-seeded-fanout` count drops and rejoins explicitly.

* **Bots at that scale.** 84 of 99 bot sessions were reset by the server
  (`Connection reset without closing handshake`) within the first minutes; the
  cause was not isolated (a plausible suspect, not verified, is the bounded
  per-session send queue overflowing when a tick takes about 0.7 s). The Python
  hosts used 5.5 cores mean (8 processes), and the surviving bots issued 14,034
  accepted and 669 rejected orders. The seeded units did not move much: bots
  ordered about 40 orders per second in total.
* **Bot play alone** (no seeding) holds real time for 99 AIs at about 600 units
  (221 orders per second, 0.8 server cores, 90 MB of Python, no bot lost), so the
  networking and the example AI are not the limit; the simulation tick at 90,000
  units is. Bots grew from 400 to 614 units in 8 minutes, nowhere near 100,000:
  the 100,000-unit scale needs the seeding hook (or a much stronger economy).
* **Browser.** Software WebGPU (SwiftShader) in the pinned Chromium with the real
  art pack at 1280 x 720 on this loaded machine: 1.1 fps around the human's
  village and 1.1 fps zoomed out in the full-scale match, against 4.4 and 7.2 fps
  for the same browser on the near-empty baseline match. Neither is a GPU
  measurement and neither reaches 30 fps; the full-scale frames are about 4 to 6
  times slower than the baseline, and the server was itself at 0.07 times real
  time, so the human's view got a tick every 0.7 s. The 30 fps target is not
  demonstrated: a real GPU, a quiet machine and a fast tick are all still missing.
* The first full-scale attempts (not kept) showed the same 0.07 speed.
  Start-site search needed two fixes found by this work: lazy candidate
  enumeration (millions of tiles per seat on a 16,000-tile map) and a far-land
  fallback for ideal points in the sea (the France square includes the Channel,
  the Atlantic and the Mediterranean).

## From the browser: France and 99 AIs through the menu

Measured 2026-10-22 at revision b882caa0 (`make dev` on port 8097, the source-backed France
package `cf552006...` reused through `AOE_PRESET_MAPS`, Chromium 1.64 with software WebGPU
driving the real menu; 1-minute load 13 to 18 from other agents). Start Game to playable took
**357 s**: the map job 4 s (reused), village placement **321 s** (100 seats, no seeding), the 99
AIs seated in **29 s** after their 8 host processes started, terrain and minimap 5 s. The
placement time sits in a few villages: the first village of several grid rows (an ideal point at
sea off the west coast) took 15 to 57 s each, the others well under a second; the server now logs
each placement over 2 s. Over 3 minutes of play with 99 bots and no human orders: tick p50 6.6 ms,
p95 24.2 ms, p99 46.7 ms (max 160), `advance` 3.6 / 16.0 ms, 43 of 3,593 ticks missed, game speed
0.996, units 674 growing to 995. Human orders (slot 0, a Python client on the human budget):
move orders were accepted within 0.1 s but the villager first moved after 43 to 220 s, and one
not within 300 s; gather orders moved within 0.9 s. That move latency is the bug being fixed on
`fix/order-latency`.

<!-- scale-match:latest -->

## Latest run: france-real-seeded-fanout

Revision `0c504af06aaaf078d0cd09924e5d0f7223dd3f65` (dirty tree), 100 players (1 human), 16000 x 16000 tiles (package), population cap 1000, 10.0 min of play, 8 Python host process(es).

| | |
|---|---|
| tick total p50 / p95 / p99 / max (ms) | 119.7 / 221.8 / 315.8 / 12477.4 |
| `advance` (simulation) p50 / p95 (ms) | 106.8 / 178.4 |
| orders, views and fan-out p50 / p95 (ms) | 13.5 / 49.9 |
| missed ticks (late start or over 50 ms) | 4132 of 4174 (99.0%) |
| game speed (1.0 = real time) | 0.347 |
| units at start / end / max | 90400 / 90235 / 90400 |
| of which seeded by `start_units` | 90000 |
| orders applied per second (mean) | 1793.127 |
| server CPU mean / max (cores), RSS max (MiB) | 2.02 / 9.077, 986 |
| bots CPU mean / max (cores), memory max (MiB) | 1.001 / 1.505, 183.72 |
| match creation / bots connected (s) | 208.4 / 16.24 |

Bots at the end: 99 reporting, 79337 villagers, 9900 soldiers, 229 buildings, 0 errors, 70983 accepted and 1006958 rejected orders. Sessions: 0 dropped and rejoined (0 drops, 0 rejoins, 0.0 s away in all), 0 permanently lost.

Browser: village 1.5 fps mean, frame p50 650.0 / p95 899.9 ms (181 frames); village_zoomed_out 1.8 fps mean, frame p50 516.7 / p95 766.7 ms (212 frames)

Per minute (tick p50 / p95 ms, missed, units at end):

| min | p50 | p95 | missed | units |
|---|---|---|---|---|
| 1 | 72.0 | 141.1 | 338 | 90400 |
| 2 | 102.1 | 147.5 | 380 | 90400 |
| 3 | 163.3 | 245.2 | 380 | 90331 |
| 4 | 166.0 | 275.9 | 380 | 90330 |
| 5 | 150.2 | 263.7 | 380 | 90289 |
| 6 | 155.5 | 281.7 | 380 | 90288 |
| 7 | 109.9 | 212.8 | 380 | 90258 |
| 8 | 102.2 | 184.6 | 380 | 90258 |
| 9 | 103.2 | 171.1 | 380 | 90239 |
| 10 | 113.3 | 192.5 | 380 | 90239 |
| 11 | 111.1 | 179.7 | 374 | 90235 |
