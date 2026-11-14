# Saved-match command replay

`make save-replay` measures recorded simulation work without Python execution,
clients, networking or a scheduled tick clock. The reference below was grown by
the ordinary Python AIs on a Mekong package of the current recipe, then saved
through the server; it describes the one revision named with it, not whatever
`dev` holds today. Saves, map packages, command streams, reports and footage stay in
`.cache/`, never Git; original game assets stay in `local-assets/`.

Run `make check-fixtures` after any save-format, snapshot-version or generation-recipe
change, and before measuring: it loads each documented package and save present in
`.cache/` and fails on one that no longer loads. `make ship` runs its report form for
map and gameplay changes, so a broken fixture shows as a warning in the evidence
without blocking unrelated work.

## Prepare current inputs

Use the [map-package workflow](../geodata/workflow.md) and the geodata already
hard-linked under `.cache/geodata`. Create
`.cache/dev-replay/mekong-request.json` with this current `MapRequest`:

```json
{
  "schema_version": 1,
  "center_latitude_e7": 145000000,
  "center_longitude_e7": 1048000000,
  "requested_side_meters": 960000,
  "compression": {"numerator": 240, "denominator": 1},
  "year_ce": 600,
  "seed": 1,
  "reconstruction_profile": "circa600_v1"
}
```

For `.cache/dev-replay/france-request.json`, use the same fields with
`center_latitude_e7: 466000000` and `center_longitude_e7: 25000000`.
Automatic rivers are the generation default. Both requests cover a 960 km
square at 1:240 compression: 2,000 × 2,000 two-metre game tiles.

```sh
make map-generate AOE_MAP_REQUEST=.cache/dev-replay/mekong-request.json AOE_MAP_PACKAGE=.cache/dev-replay/packages
make map-verify AOE_MAP_PACKAGE=.cache/dev-replay/packages/4d9cb16c1c286a5b29ba1d94e70371af966ac7a130a1c3c0f6bcbe95f3b5b683.json
make map-generate AOE_MAP_REQUEST=.cache/dev-replay/france-request.json AOE_MAP_PACKAGE=.cache/dev-replay/packages
make map-verify AOE_MAP_PACKAGE=.cache/dev-replay/packages/fdc59853e2404cb5c7fb245f21768970aeb01febe20fd4f15dc96ff522fc9a49.json
```

Both were freshly generated and verified at recipe 4. France was inexpensive
from the cache and is retained as an optional input; only Mekong was played
and measured. Mekong's manifest binds 90 source locks, including its elevation
refinement; the package verifier checks the published pages.

Use asset pack `8f7abe8c330d7fd710daa076c8ef07de65f8973feed59b99f4d01f9cbc9c4530`
under the canonical `local-assets/packs` root. Its existing `buildings` link is
relative to sibling pack `3db32320fb1e15d3ccdeb6b00290e401d54714b074fadca23687c4650dd47769`,
whose `actors` link is relative to sibling
`2db1eba05188acc065bec338e3426a2bd23ffd7d4823ef3e0a0ec81fa2cbaaa6`.
All three must be accessible in the worktree; no absolute links or original
assets are copied into images or Git.

## Grow and save the war state

For a fresh `target/scale-match` path, keep scratch output under `.cache/`:

```sh
mkdir -p .cache/dev-replay/work target
ln -s ../.cache/dev-replay/work target/scale-match
make build-wasm
make save-replay-counter-check
make scale-match SCALE_MATCH_ARGS='--players 100 --humans 1 --tiles 2000 --map package --map-hash 4d9cb16c1c286a5b29ba1d94e70371af966ac7a130a1c3c0f6bcbe95f3b5b683 --package-directory .cache/dev-replay/packages --asset-pack local-assets/packs/8f7abe8c330d7fd710daa076c8ef07de65f8973feed59b99f4d01f9cbc9c4530 --hosts 8 --minutes 150 --port 9288 --label current-mekong --no-summary --out .cache/dev-replay/growth.json'
```

Do not replace an existing scratch directory or symlink. The 150-minute value
is a wall-clock safety ceiling. No extra units were seeded: 400 starting units
grew through ordinary play by 99 Python AIs and one idle human seat. All AIs
connected in 3.5 seconds. Save after reaching the desired game-time and war
state, using the game menu or `POST /saves` with the human's
`X-AoeWorld-Controller-Token` and `{"name":"Mekong dev late"}`.
The local adapter used for that API call is retained with its SHA-256 in
`.cache/rec-inputs.md`:

```sh
python3 .cache/dev-replay/control.py 'Mekong dev late'
```

The chosen save is
`.cache/dev-replay/packages/saves/1794572640-mekong-dev-late.save`, with its
sibling JSON card. It contains snapshot 5 at tick **72,380**, **60.32 game
minutes**, **10,761 units**, **5,765 buildings**, and **99 opponent seats**.
The adjacent admin observation at tick 72,381 has 7,069 cumulative player
kills and `combat_per_minute` counters of 344 deaths, 6,967 melee hits and
10,083 ranged hits. Sixty-one players were Imperial, 17 Castle, 20 Feudal
and two Dark. The idle human and two AIs were eliminated; all 100 seats remain
in the save, and defeated AI seats still connect normally.

The growth server was deliberately stopped after saving and filming. Its
`growth.json.failure` records that stop; it is not a completed 150-minute
performance baseline. The save was sealed before fog was disabled for footage.
Preserve growth logs before the saved capture reuses the scratch path:

```sh
mv .cache/dev-replay/work .cache/dev-replay/growth-work
mkdir .cache/dev-replay/work
```

## Capture and replay the reference

```sh
make scale-match SCALE_MATCH_ARGS='--save .cache/dev-replay/packages/saves/1794572640-mekong-dev-late.save --package-directory .cache/dev-replay/packages --asset-pack local-assets/packs/8f7abe8c330d7fd710daa076c8ef07de65f8973feed59b99f4d01f9cbc9c4530 --record-commands .cache/dev-replay/commands.jsonl --record-ticks 1500 --minutes 30 --port 9288 --no-summary --out .cache/dev-replay/record.json'
make save-replay AOE_SAVE_REPLAY_ARGS='--save .cache/dev-replay/packages/saves/1794572640-mekong-dev-late.save --package-directory .cache/dev-replay/packages --recording .cache/dev-replay/commands.jsonl --runs 3 --out .cache/dev-replay/replay.json'
```

The capture succeeded with all **99 AIs ready across eight hosts** in 5.855
seconds before the saved clock resumed. It contains **10,055 commands** over
exactly **1,500 contiguous input ticks, 72,380–73,879**; the final resulting
state is tick 73,880. The 30-minute value is a safety deadline: capture stops
when the requested ticks are complete. Output recordings refuse overwrite and
partial streams fail. Select the stored save directly under `packages/saves/`
when reusing a package directory; importing a different copy refuses overwrite.

Recording is armed before the restored service is installed. The restored clock
stays paused until every ordinary skirmish AI seat connects, then resumes once.
The human socket sends no orders. Every applied `PendingAction`, including
rejected actions, retains its player, tick and actual application order.
Current `Move.attack_move`, queue flags and external Steward, Vassal, General,
rally and other settings in this path are captured. Their internal deterministic
decisions remain simulation work. Unrelated admin/scenario mutations are outside
the stream's contract; the observer fog toggle was confined to the earlier
source match, after its reference save, and is absent from this capture.

Replay uses a private `.cache/save-replay/target` per worktree even when
`HARNESS_TARGET_CACHE` is shared, preventing another worktree's Cargo freshness
metadata from selecting a stale simulation library through the normalized source
alias. The content-addressed compiler object cache can still be reused.

## Measured reference

These figures describe runtime revision
`dfad610ac7af6e5b0266bfa800cb4d460dffc056` (measured 2026-11-13, with only the
documentation warning dirty). They are **not** a running baseline for whatever
`dev` currently holds: merges after that revision, such as the route planner
and the castle/Town Center combat changes, are not included, and each later
merge moves the number. The save, packages and command stream stay valid
across those merges, so re-run the command above on the revision you care
about and compare against your own run rather than against this table.

The report's source-diff BLAKE3 is
`2f71987586a110f74a3ac14eaaeb6e51ff0f78c1e539d595dcfb9de09316177d`;
the diff is retained in `.cache/dev-replay/measurement-source.diff`.
No gameplay or measurement-tool code changed. Results were added afterward.
There was one measurement invocation, three independent restores and
1,500-tick replays. The raw report retains every paired instruction/CPU sample,
per-phase percentile, resulting canonical hash and first-divergence tick.

| Run | Mean instructions/tick | p50 | p95 | Mean thread CPU ms/tick | Instructions/tick/restored unit |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 391,450,081.31 | 339,850,631 | 705,039,504 | 179.876 | 36,376.74 |
| 2 | 391,461,182.03 | 339,877,030 | 705,000,374 | 161.216 | 36,377.77 |
| 3 | 391,490,955.67 | 339,831,370 | 704,897,736 | 143.132 | 36,380.54 |
| Mean of run means | 391,467,406.34 | — | — | 161.408 | 36,378.35 |

Instruction spread: **0.0104%**. Thread CPU-time spread:
**22.7647%**. Spread is `(max run mean - min run mean) / average run mean * 100`.
Normalization uses the **10,761 initially restored units**, with
**5,765 buildings** retained alongside that count. Population can
change during the window; this is not normalization by each tick's live count.

| Phase | Mean instructions/tick | Three-run spread |
| --- | ---: | ---: |
| `apply_loop` | 43,874,821.92 | 0.0017% |
| `attack_position_search` | 13,872,887.48 | 0.0042% |
| `building_memory` | 664,107.66 | 0.0344% |
| `changed` | 656,550.32 | 3.0885% |
| `combat` | 23,032,766.64 | 0.0019% |
| `components` | 2,067,752.04 | 0.0305% |
| `construction` | 3,314,392.98 | 0.0052% |
| `gathering` | 26,990,639.08 | 0.0075% |
| `group_apply` | 19,889,313.73 | 0.0016% |
| `group_prepare` | 9,488,938.85 | 0.0004% |
| `group_propose` | 22,082,081.75 | 0.0001% |
| `hierarchy` | 10,553,488.06 | 0.0140% |
| `index_chunks` | 25,765.21 | 0.0142% |
| `index_collision` | 2,793,313.79 | 0.0005% |
| `local_refinement` | 910,804.98 | 0.0083% |
| `movement` | 294,974,041.92 | 0.0048% |
| `orders` | 15,505,362.24 | 0.0053% |
| `plan_ahead` | 6,773,963.58 | 0.0121% |
| `plan_fair` | 87,952,395.89 | 0.0079% |
| `plan_fast` | 75,084,411.60 | 0.0040% |
| `plan_serve` | 141,066,113.45 | 0.0069% |
| `pocket` | 6,790,814.76 | 0.0015% |
| `portal_costs` | 2,164,567.89 | 0.0033% |
| `portals` | 5,917,412.05 | 0.0073% |
| `propose` | 45,724,775.29 | 0.0059% |
| `research_training` | 1,361,309.49 | 0.0162% |
| `route_cache` | 419,134.88 | 0.0047% |
| `search` | 37,306,569.29 | 0.0017% |
| `sidestep` | 22,085,897.35 | 0.0004% |
| `spacing` | 1,383,842.18 | 0.0101% |
| `steward_economy` | 21,570,804.59 | 0.0188% |
| `terrain_point` | 151,871,868.14 | 0.0123% |
| `vision_flush` | 3,197,024.61 | 0.0028% |
| `wildlife` | 1,119,315.24 | 0.0090% |

Phases zero in all three runs: `delivery`, `fanout_wait`, `frame`, `observation_lock`, `prefetch`, `resource_journal`, `resources`, `session_views`, `sessions`, `terrain`, `viewers`, `walk_cache`, `world_lock`.
Inclusive spans overlap; these rows must not be summed.

Initial hash, all three runs and capture:
`f7eb96449f4de09c82bb29fe8f8e79d63816eb93ca192f2ade7e1fca1482724d`.
Final hash, all three runs and capture:
`807ae7ff6a8491cb51009c415adc0cd1b346f251c655f77fb43a232c8c153b14`.
Every per-tick hash agrees across all three replays and the capture; first
divergence: none in every run. Hash agreement is observed evidence, not a
requirement to preserve superseded gameplay.

Save BLAKE3: `2c35ae4d2f711a3bad545cb020acc69b5461d63119173f345ed92cdeba896a78`.
Recording BLAKE3: `3b7cd6836ff9f2b8632237b45e4fd9bfdbe4ab2c69b0966eef689694a80b39e6`.

All three restores report:

```text
RestoreReport { buildings: 5765, units: 10761, buildings_lost: 0, units_lost: 0, jobs_resumed: 7096, jobs_lost: 170, animals: 478, animals_lost: 0, formations: 531, managers: 52 }
```

All units, buildings and animals restored. The ordinary loader dropped 170
jobs before timing; those losses are retained here rather than hidden.

## Counter and qualification limits

`make save-replay-counter-check` passed the real PMU thread-ownership,
sleep-exclusion and complete-scheduling test. Both it and replay request Docker
`--cap-add PERFMON` with the default seccomp profile. Host policy and PMU
availability remain prerequisites; unavailable counters, failed reads,
wrong-thread reads and multiplexing fail instead of emitting estimates.

The Linux adapter measures `PERF_COUNT_HW_INSTRUCTIONS` on the simulation
thread, `pid=0`, any CPU, inheritance off, kernel/hypervisor instructions
excluded, with one Rayon worker. A pinned event must have equal nonzero
running/enabled times on every tick. Paired CPU time uses
`CLOCK_THREAD_CPUTIME_ID` on that worker and includes its kernel CPU work.
Probe overhead within tick spans remains counted. Counter setup, restore and
navigation warmup, canonical hashing, bookkeeping, AI execution, fanout,
transport and save/recording I/O are outside the measured tick; lazy terrain
work remains counted.

Instructions describe user-space work, not latency, live tick rate, parallel
speedup or finished RTS performance. The 0.0104% instruction spread is well
under the repeatability aim of 1%; the 22.7647% CPU-time spread also records
shared-host frequency/cache/runtime variation. Neither metric nor this
10,761-unit workload qualifies real time at 100 players/100k units,
1000 players/1M units or 1000 players/10M units, or dedicated hardware.

## Local evidence and historical inputs

`.cache/rec-inputs.md` lists exact absolute input paths and SHA-256s, with a
full page/asset inventory for other sessions to hard-link. Keep the save/card,
package pages/navigation and relative pack links together. Reports and inputs
remain private ignored artifacts.

Local footage is `.cache/showcase/current-dev-war.webm`: 39.64 seconds,
1280×720 VP9, decoded and visually inspected. It shows the source match after
the reference save with fog disabled for observation; the 0/0 Dark Age HUD
belongs to the eliminated human. It is outside the captured command window and
instruction measurement. It was recorded with `make showcase` using
`.cache/showcase/current-dev-war.json`, then trimmed from the full take;
`make video-probe` verified the final encoding and duration. Attach the final
clip later with `SHIP_VIDEO`; never commit footage or original game assets.

The old mid (`1794451672-mekong.save`) and late
(`1794535945-mekong-2.save`) windows require a **pre-#800 checkout**, also before
#801: they use snapshot 3, recipe 3 and command streams without
`Move.attack_move`. They are not current-dev inputs. Regenerate inputs instead
of adding compatibility readers. The fresh window is a different workload;
its cost is not an optimization speedup against those obsolete windows.
