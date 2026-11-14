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
make scale-match SCALE_MATCH_ARGS='--players 100 --humans 1 --tiles 2000 --map package --map-hash 4d9cb16c1c286a5b29ba1d94e70371af966ac7a130a1c3c0f6bcbe95f3b5b683 --package-directory .cache/dev-replay/packages --asset-pack local-assets/packs/8f7abe8c330d7fd710daa076c8ef07de65f8973feed59b99f4d01f9cbc9c4530 --hosts 8 --minutes 150 --port 9288 --label rerecord-mekong --no-summary --out .cache/dev-replay/growth.json'
```

Do not replace an existing scratch directory or symlink. The 150-minute value
is a wall-clock safety ceiling. No extra units were seeded: 400 starting units
grew through ordinary play by 99 Python AIs and one idle human seat. All AIs
connected in 3.5 seconds. Save after reaching the desired game-time and war
state, using the game menu or `POST /saves` with the human's
`X-AoeWorld-Controller-Token` and `{"name":"Mekong dev late v6"}`.
The local adapter used for that API call is retained with its SHA-256 in
`.cache/rec-inputs.md`:

```sh
python3 .cache/dev-replay/control.py 'Mekong dev late v6'
```

The chosen save is
`.cache/dev-replay/packages/saves/1794634126-mekong-dev-late-v6.save`, with its
sibling JSON card. It contains snapshot 6 at tick **72,451**, **60.37 game
minutes**, **9,667 units**, **5,381 buildings**, and **99 opponent seats**.
Its SHA-256 is
`c9577a096975b60de094f41ea674f04e6325d855256283ff918c4986bd3afba8`, the card's
`8de4f3f5062a7979e26163e99e89d3add0edd6b81ce4e2bf2e9ebab15eff855c`, and the
Mekong package manifest's
`36494a063c0f62d3c8d0bcbdf54dc3ccef8966c5cf40a8af6c9d634dc46d876e`, so
`make check-fixtures` has a current list to read.
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
make scale-match SCALE_MATCH_ARGS='--save .cache/dev-replay/packages/saves/1794634126-mekong-dev-late-v6.save --package-directory .cache/dev-replay/packages --asset-pack local-assets/packs/8f7abe8c330d7fd710daa076c8ef07de65f8973feed59b99f4d01f9cbc9c4530 --record-commands .cache/dev-replay/commands.jsonl --record-ticks 1500 --minutes 30 --port 9288 --no-summary --out .cache/dev-replay/record.json'
make save-replay AOE_SAVE_REPLAY_ARGS='--save .cache/dev-replay/packages/saves/1794634126-mekong-dev-late-v6.save --package-directory .cache/dev-replay/packages --recording .cache/dev-replay/commands.jsonl --runs 3 --out .cache/dev-replay/replay.json'
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
`269c69d6fde4b75d8697ec86fd3cfe952e4e3c65`, measured 2026-11-14 on a clean
tree. They are **not** a running baseline for whatever `dev` currently holds:
each later merge moves the number. The save, packages and command stream stay
valid across merges, so re-run the command above on the revision you care
about and compare against your own run rather than against this table.

**This reference starts a new series.** The previous one
(`1794572640-mekong-dev-late.save`, 391.5M down to 251.3M instructions/tick
across 2026-11-13) became unreadable when #828 added
`FormationRecord::pace_limit` without bumping `WORLD_SNAPSHOT_VERSION`; see
#839, which bumped the snapshot to 6 and the save to 2 and made the failure
say so. That save cannot be re-measured, and this one is a different world
state with 9,667 units against 10,761, different building and army
composition. **Do not subtract the two.** The earlier improvements were valid
comparisons on identical inputs at the time they were taken; nothing about
them can be confirmed or denied from the table below.

The report's source-diff BLAKE3 is
`af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262` (an empty
diff: the tree was clean). The save's BLAKE3 is
`aa555443c84d44e59ddb91723093f79e7fc3655ff3b741aabad2c20c2a493d20` and the
recording's `7096491314985242a2669f1d63f4f66491b59d07bf74853912d29746ebeaa90e`.
One measurement invocation, three independent restores and 1,500-tick replays.
All three produced the same final canonical hash
`e816c9e6475a398ec5289b4daa71811320a2639356a642a29f5c326556973a14` with no
divergence tick, and the recording applied 9,644 commands in each run.

| Run | Mean instructions/tick | p50 | p95 | Mean thread CPU ms/tick | Instructions/tick/restored unit |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 241,749,297.46 | 222,037,665 | 364,585,398 | 140.713 | 25,007.69 |
| 2 | 241,752,335.66 | 222,047,113 | 364,488,511 | 129.386 | 25,008.00 |
| 3 | 241,759,285.32 | 221,988,424 | 364,490,445 | 154.933 | 25,008.72 |
| Mean of run means | 241,753,639.48 | — | — | 141.677 | 25,008.13 |

Instruction spread: **0.0041%**. Thread CPU-time spread: **18.0318%**. Spread
is `(max run mean - min run mean) / average run mean * 100`. Normalization uses
the **9,667 initially restored units**, with **5,381 buildings** retained
alongside that count. Population can change during the window; this is not
normalization by each tick's live count.

Phase instructions per tick, mean of the three runs. Phases nest, so they do
not sum to the total: the planning and search phases run inside movement's
span, and `late` covers work after the unit frame.

| Phase | Mean instructions/tick | Three-run spread |
| --- | ---: | ---: |
| `movement` | 149,905,533 | 0.0007% |
| `plan_serve` | 47,726,763 | 0.0019% |
| `combat` | 38,348,723 | 0.0046% |
| `plan_fast` | 36,089,455 | 0.0016% |
| `apply_loop` | 36,003,229 | 0.0003% |
| `search` | 33,255,504 | 0.0018% |
| `steward_economy` | 22,184,589 | 0.0040% |
| `propose` | 19,999,517 | 0.0003% |
| `plan_fair` | 18,929,827 | 0.0015% |
| `terrain_point` | 15,785,010 | 0.0081% |
| `group_propose` | 14,855,869 | 0.0005% |
| `sidestep` | 14,789,814 | 0.0002% |
| `group_apply` | 14,705,543 | 0.0004% |
| `gathering` | 14,410,490 | 0.0011% |
| `hierarchy` | 13,470,284 | 0.0036% |
| `issuer_vassal` | 12,504,219 | 0.0073% |
| `attack_position_search` | 11,780,924 | 0.0042% |
| `orders` | 8,933,251 | 0.0020% |

Movement remains the dominant phase at 62% of the tick.

All three restores report:

```text
RestoreReport { buildings: 5381, units: 9667, buildings_lost: 0, units_lost: 0, jobs_resumed: 6456, jobs_lost: 118, animals: 453, animals_lost: 0, formations: 501, managers: 55 }
```

All units, buildings and animals restored. The ordinary loader dropped 118
jobs before timing; those losses are retained here rather than hidden.

### Live tick rate of the source match

The capture run that produced the command stream reported, from the live server
with all 99 Python AI hosts on the same machine, **7.57 ticks/s** at 9,730
units: total tick 120.6 ms p50 and 216.9 ms p95, with `movement` 48.8 ms p50 of
that and `late` 98.3 ms p50. This is a shared-host wall-clock observation
alongside the bot hosts, not a counter measurement, and it is **indicative
only**; the instruction table above is the repeatable figure. It is recorded
because it is the closest thing here to what a player experiences at this
population.

The same capture reported route-planning starvation worth reading before
optimizing anything else: 66 of 72 `attack_approach` requests were `unplanned`
with waits up to 700 ticks, `oldest_wait_ticks` 1,308, `players_at_search_cap`
34, `slot_refusals` 53,654, and all 179 detour requests declined
(`detours_at_capacity` 179). Armies ordered to attack were waiting for routes.

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
speedup or finished RTS performance. The 0.0041% instruction spread is well
under the repeatability aim of 1%; the 18.0318% CPU-time spread also records
shared-host frequency/cache/runtime variation. Neither metric nor this
9,667-unit workload qualifies real time at 100 players/100k units,
1000 players/1M units or 1000 players/10M units, or dedicated hardware.

## Local evidence and historical inputs

`.cache/rec-inputs.md` lists exact absolute input paths and SHA-256s, with a
full page/asset inventory for other sessions to hard-link. Keep the save/card,
package pages/navigation and relative pack links together. Reports and inputs
remain private ignored artifacts.

No footage was taken of this reference. The previous window's local clip
`.cache/showcase/current-dev-war.webm` described below belongs to the obsolete
save and is kept only as a record: 39.64 seconds,
1280×720 VP9, decoded and visually inspected. It shows the source match after
the reference save with fog disabled for observation; the 0/0 Dark Age HUD
belongs to the eliminated human. It is outside the captured command window and
instruction measurement. It was recorded with `make showcase` using
`.cache/showcase/current-dev-war.json`, then trimmed from the full take;
`make video-probe` verified the final encoding and duration. Attach the final
clip later with `SHIP_VIDEO`; never commit footage or original game assets.

Every earlier window is obsolete and none can be read by current code. The old
mid (`1794451672-mekong.save`) and late (`1794535945-mekong-2.save`) windows use
snapshot 3 and recipe 3; `1794572640-mekong-dev-late.save` uses snapshot 5,
which #828 made unreadable before #839 named the break. They are not
current-dev inputs and there is no reader for them. Regenerate inputs instead
of adding compatibility readers. The fresh window is a different workload;
its cost is not an optimization speedup against those obsolete windows.
