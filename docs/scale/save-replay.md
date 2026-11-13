# Owner-save command replay

`make save-replay` measures the same recorded simulation workload without Python
AIs, clients, networking or a scheduled tick clock. Inputs and reports belong in
`.cache/`, never Git. Copy the save, its sibling card and its map package/pages
there before recording; do not modify the owner's originals.

Record once through the ordinary saved skirmish (choose an unused local port):

```sh
make scale-match SCALE_MATCH_ARGS='--save .cache/save-bench/input/1794451672-mekong.save --package-directory .cache/save-bench/packages --record-commands .cache/save-replay/commands.jsonl --record-ticks 1500 --minutes 30 --port 9288 --no-summary --out .cache/save-replay/record.json'
```

The 30-minute value is a safety deadline, not a benchmark duration: this run
stops when the 1,500 recorded ticks are complete. On subsequent invocations,
select the imported save directly under `packages/saves/` or use a fresh package
directory; the importer refuses to overwrite saves. The recording itself also
refuses overwrite. A partial recording is rejected rather than silently used.

Recording is armed before the restored service is installed. Every applied
`PendingAction`, including rejected actions, retains its player, tick and actual
application order. It covers Python AI commands and all external Steward,
Vassal, General, rally and other settings in this command path. Their internal
deterministic decisions remain simulation work during replay. No synthetic
replacement AI is used. This capture mode is for the saved-skirmish harness,
whose human socket sends no commands; unrelated admin/scenario mutations are
outside its input contract.

During saved recording only, the restored clock stays paused until the ordinary
skirmish fleet reports all AI seats connected. Readiness resumes this clock once;
ordinary matches and later manual pauses retain their existing behavior.

Replay from the same initial save:

```sh
make save-replay AOE_SAVE_REPLAY_ARGS='--save .cache/save-bench/input/1794451672-mekong.save --package-directory .cache/save-bench/packages --recording .cache/save-replay/commands.jsonl --out .cache/save-replay/report.json'
```

Replay builds use a private `.cache/save-replay/target` in each worktree, even
when `HARNESS_TARGET_CACHE` is shared. This prevents Cargo freshness metadata
from accepting another worktree's simulation library through the normalized
source alias. The content-addressed compiler object cache can still be reused.

The default is three independent restores and replays in one measurement
invocation (`--runs 1` through `10` is supported). Use the same recording on each
revision's worktree, with this measurement adapter applied identically. Reports
identify HEAD, dirty status and the recording's BLAKE3 digest. Retain the adapter
diff alongside results when measuring revisions predating the command.

The report contains total and per-phase mean/p50/p95 for retired instructions
and thread CPU milliseconds, raw paired samples, every resulting canonical hash, final hash and the first
divergence tick. Initial-state disagreement reports the initial tick. Later
divergence does not suppress instruction results or stop replay. Tick gaps and incomplete
streams fail. Spread is `(largest mean - smallest mean) / average mean * 100`.

The Linux native adapter uses `perf-event2` / `perf_event_open` with
`PERF_COUNT_HW_INSTRUCTIONS`, current thread (`pid=0`), any CPU, inheritance off,
and kernel/hypervisor instructions excluded. A counter is opened on the simulation
thread for each tick. One Rayon worker executes the tick and its proposal work.
Both metrics are always reported together. CPU time uses Linux
`CLOCK_THREAD_CPUTIME_ID` on the same worker; unlike the user-only instruction
event, this clock includes the thread's kernel CPU work. The PMU event is pinned
(not CPU affinity); each tick
must have equal nonzero enabled/running times. Unavailable counters, failed reads,
wrong-thread reads and multiplexing fail rather than emit estimated counts.

Check the real PMU before running the owner-save measurement:

```sh
make save-replay-counter-check
```

Both this check and `save-replay` request Docker `--cap-add PERFMON`, retaining
the default seccomp profile. On the measured host (`perf_event_paranoid=1`), the
default container was denied and PERFMON alone allowed the event; an unconfined
seccomp profile was unnecessary. Host policy and PMU availability still matter;
access failures explain the prerequisite. Ordinary unit tests use a fixture
counter; the explicit hardware check verifies live counting, thread ownership,
sleep exclusion and complete PMU scheduling.

Paired instruction/CPU spans reuse the running-match profiler's instrumentation points.
Inclusive navigation/movement spans overlap: do not sum percentile rows.
Probe overhead is included consistently; counter setup, canonical hashing,
restore/navigation warmup, bookkeeping, fanout, AI execution and save/recording
I/O are outside the counted tick. Lazy terrain work remains counted.

The instruction metric reports user-space work, not latency, live ticks/s, parallel speedup,
finished RTS performance or dedicated-hardware qualification. Descheduling does
not add instructions. Runtime/allocator paths can still vary, so report three-run
instruction spread; the repeatability aim is well under 1%. CPU spread is reported
separately and remains sensitive to host frequency/cache conditions. Raw integer samples and integer
percentiles retain exact units; means are unscaled instructions per tick.

## Late war window and comparison

Copy `1794535945-mekong-2.save` and its sibling `.json` card from the owner's
evidence directory into `.cache/save-bench-late/input/`. Never edit or move the
originals. Copy the Mekong package `2976b04b…` and its `pages/` and `navigation/`
directories into `.cache/save-bench-late/packages/`. Hard-link the local asset
pack `8f7abe8c…` and its referenced building pack under
`.cache/save-bench-late/packs/`; convert any absolute symlinks to relative ones
so the container can resolve them. Assets, saves, command streams, video and raw
reports stay ignored under `.cache/`.

The server validates packs against the canonical `local-assets/packs` root. On
a fresh worktree, expose the copied set with
`ln -s ../.cache/save-bench-late/packs local-assets/packs` after creating
`local-assets/`; the pack files still live under `.cache/`. Do not replace an
existing pack directory. For scratch logs too, create
`.cache/save-bench-late/work/` and link `target/scale-match` to
`../.cache/save-bench-late/work` before running the harness.

The card describes tick 588,067, 99 opponents, 8,573 units and 12,423 buildings.
The human's population and town-center count describe one player; benchmark
normalization uses all restored units. Record exactly once, with all 99 ordinary
Python AIs connected:

```sh
make scale-match SCALE_MATCH_ARGS='--save .cache/save-bench-late/input/1794535945-mekong-2.save --package-directory .cache/save-bench-late/packages --asset-pack .cache/save-bench-late/packs/8f7abe8c330d7fd710daa076c8ef07de65f8973feed59b99f4d01f9cbc9c4530 --record-commands .cache/save-bench-late/commands.jsonl --record-ticks 1500 --minutes 30 --port 9288 --no-summary --out .cache/save-bench-late/record.json'
make save-replay AOE_SAVE_REPLAY_ARGS='--save .cache/save-bench-late/input/1794535945-mekong-2.save --package-directory .cache/save-bench-late/packages --recording .cache/save-bench-late/commands.jsonl --runs 3 --out .cache/save-bench-late/replay.json'
```

For optional in-game footage, prepare `make build-wasm` and `make browser-deps`
before recording. While the saved match is ready, use `make showcase` with
`SHOWCASE_APP_URL=http://127.0.0.1:9288` and a browser scene at `/?play=1`
([storyboards](../showcase.md)). Saved matches reject `--browser`; the separate
recorder connects to the human seat. Footage is outside the instruction
measurement, which always replays headlessly.

Copy the earlier `.cache/save-bench/` inputs and
`.cache/save-replay/commands.jsonl` from the original benchmark worktree, naming
the latter `.cache/save-replay/commands-mid.jsonl`, then measure once:

```sh
make save-replay AOE_SAVE_REPLAY_ARGS='--save .cache/save-bench/input/1794451672-mekong.save --package-directory .cache/save-bench/packages --recording .cache/save-replay/commands-mid.jsonl --runs 3 --out .cache/save-replay/mid.json'
```

Compare the average of the three run means, each run's mean, per-phase means,
spread, restored counts, initial/final hashes and divergence ticks. Divide each
window's instructions/tick by its restored unit count to get instructions per
tick per unit; retain building counts alongside it because buildings also cost
work. These are different game states and command streams, so their ratio is a
workload comparison, not an optimization speedup. Hash disagreement is reported
as evidence; it is not a reason to preserve superseded gameplay. Neither window
qualifies the 100-player/100k-unit, 1000-player/1M-unit or 1000-player/10M-unit
real-time scale targets.


## Measured windows on this branch

Measured at `e4edfb9db67344e4953a7d1397a7d823d10b0f5b` plus the benchmark
port, before its commit. Both reports identify the same source diff BLAKE3
`06584aaa6b116b1ee4a65e278ca56101051482c08981bcc4c391c9592bc5cb39`.
The measured production diff SHA-256 is
`0cda2b5bff85ef7e745ff70f5ca890b6a65607ad9682727431a4127d5395616d`;
only documentation/results were added afterward. Raw reports are
`.cache/save-replay/mid.json` and `.cache/save-bench-late/replay.json`.
One measurement invocation per window, three 1,500-tick runs each.

| Window | Run 1 instructions/tick | Run 2 | Run 3 | Mean | Spread | Units / buildings restored | Instructions/tick/unit |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Mid | 391,768,962.50 | 391,771,935.00 | 391,781,403.26 | 391,774,100.25 | 0.0032% | 11,756 / 7,632 | 33,325.46 |
| Late | 217,162,374.89 | 217,165,308.44 | 217,170,706.72 | 217,166,130.02 | 0.0038% | 8,573 / 12,423 | 25,331.40 |

Per-phase means across the three runs (instructions/tick); spread uses the
same `(max - min) / mean` formula on each phase’s run means. Inclusive spans
overlap, so these rows must not be summed.

| Phase | Mid mean | Mid spread | Late mean | Late spread |
| --- | ---: | ---: | ---: | ---: |
| `apply_loop` | 45,023,961.28 | 0.0011% | 15,769,634.84 | 0.0021% |
| `attack_position_search` | 33,272,374.62 | 0.0081% | 2,677,658.11 | 0.0045% |
| `building_memory` | 284,053.37 | 0.0458% | 86,386.38 | 0.0355% |
| `changed` | 644,618.03 | 1.2807% | 324,688.03 | 0.9750% |
| `combat` | 36,464,920.00 | 0.0043% | 8,921,818.40 | 0.0014% |
| `components` | 1,794,337.71 | 0.0180% | 1,135,502.69 | 0.0082% |
| `construction` | 5,435,596.83 | 0.0056% | 7,980,224.61 | 0.0036% |
| `gathering` | 11,233,177.71 | 0.0027% | 10,720,857.81 | 0.0035% |
| `group_apply` | 71,014,145.10 | 0.0018% | 26,170,752.92 | 0.0025% |
| `group_prepare` | 25,222,520.42 | 0.0010% | 11,610,333.37 | 0.0021% |
| `group_propose` | 51,247,674.07 | 0.0005% | 23,626,827.55 | 0.0001% |
| `hierarchy` | 24,244,351.09 | 0.0090% | 5,610,102.02 | 0.0037% |
| `index_chunks` | 34,530.41 | 0.0101% | 23,701.50 | 0.0049% |
| `index_collision` | 2,676,217.67 | 0.0002% | 1,208,383.06 | 0.0002% |
| `local_refinement` | 341,974.55 | 0.0049% | 238,938.44 | 0.0008% |
| `movement` | 303,319,985.26 | 0.0010% | 169,407,881.54 | 0.0014% |
| `orders` | 18,180,023.88 | 0.0112% | 6,148,127.41 | 0.0023% |
| `plan_ahead` | 2,174,645.66 | 0.0023% | 1,454,135.17 | 0.0127% |
| `plan_fair` | 33,217,899.20 | 0.0003% | 38,065,398.64 | 0.0016% |
| `plan_fast` | 64,142,801.84 | 0.0022% | 53,390,751.67 | 0.0016% |
| `plan_serve` | 81,687,869.62 | 0.0022% | 77,615,419.18 | 0.0015% |
| `pocket` | 16,178,075.45 | 0.0020% | 13,946,673.80 | 0.0011% |
| `portal_costs` | 1,434,991.43 | 0.0058% | 893,022.01 | 0.0067% |
| `portals` | 5,084,762.99 | 0.0058% | 2,810,560.03 | 0.0074% |
| `propose` | 17,244,132.12 | 0.0008% | 8,325,900.47 | 0.0030% |
| `research_training` | 46,618.69 | 0.0326% | 43,745.88 | 0.0311% |
| `route_cache` | 527,957.38 | 0.0024% | 141,572.97 | 0.0100% |
| `search` | 57,950,448.74 | 0.0016% | 40,418,513.54 | 0.0016% |
| `sidestep` | 66,218,420.55 | 0.0001% | 19,323,956.57 | 0.0001% |
| `spacing` | 1,777,444.06 | 0.0004% | 844,178.00 | 0.0014% |
| `steward_economy` | 10,890,945.01 | 0.0056% | 10,921,496.86 | 0.0241% |
| `terrain_point` | 30,854,646.56 | 0.0028% | 35,015,859.51 | 0.0049% |
| `vision_flush` | 3,351,182.10 | 0.0010% | 1,815,101.22 | 0.0006% |
| `wildlife` | 1,811,348.84 | 0.0030% | 968,109.85 | 0.0040% |

Counters that remain zero in both headless windows: `delivery`, `fanout_wait`, `frame`, `observation_lock`, `prefetch`, `resource_journal`, `resources`, `session_views`, `sessions`, `terrain`, `viewers`, `walk_cache`, `world_lock`.

Mid: recording BLAKE3 `26df5866f47d1818fc7719b2ccf5fd0db796f5a9de9b2d3355ef3f39a343ea3d`;
save BLAKE3 `be0229478c8fa849657c1512a41d96a937ba06620c608f182b1ac803fdaf3d4a`.
Repeat hash streams agree: `true`.
Initial hash, all three runs: `184046864036435ffc58a39578dd381add731ac8f32e917924cd676292a0386c`.
Final hash, all three runs: `c7de659bf09f5585e8b05ea5d58ae683a8fcced4faa7dcc01ea59ccc168225e6`.
First divergence ticks from capture: 184362, 184362, 184362.
Restoration: `RestoreReport { buildings: 7632, units: 11756, buildings_lost: 0, units_lost: 0, jobs_resumed: 3451, jobs_lost: 52, animals: 412, animals_lost: 0, formations: 1424, managers: 13 }`.
CPU mean ms/tick by run: 307.878, 286.26, 286.75; CPU spread 7.3623%.

Late: recording BLAKE3 `7cebcd87eb6dc948db0d8800b00dcfa34ca9a263cd6f8bb9d330ab059e92d7b5`;
save BLAKE3 `cdb3d3b99198704d0ed8e5146f2fd741e03bc016accc9df93aad9550dd5a3b4c`.
Repeat hash streams agree: `true`.
Initial hash, all three runs: `e05bf0af0dc25553025d777f77aaa1e66ab7046891094a07c34f3548c12b3e93`.
Final hash, all three runs: `805f4b197d39581f9eb164a6994ad18927ec1c038f982ce08abc9b5156d4af2e`.
First divergence ticks from capture: none in any run.
Restoration: `RestoreReport { buildings: 12423, units: 8573, buildings_lost: 0, units_lost: 0, jobs_resumed: 1744, jobs_lost: 42, animals: 351, animals_lost: 0, formations: 967, managers: 20 }`.
CPU mean ms/tick by run: 136.324, 118.853, 129.413; CPU spread 13.6283%.

The late capture contains 4,763 applied commands, with 99 AIs ready across eight
hosts before its clock resumed. Its three replay hash streams match the capture
through the entire window. The historical mid capture differs already at its
initial state; its three current replay streams agree with one another. No
simulation behavior was changed by this tooling port.

Every unit and building restored in both windows. The ordinary loader dropped
52 mid-game jobs and 42 late-game jobs during restore, outside timing; the full
restore reports above retain those losses. Movement accounts for about 77.4%
of mid-game and 78.0% of late-game instructions. These are workload costs on
this branch, not evidence of an optimization between the windows.

Local footage: `.cache/showcase/late-war.webm` (38.4 seconds, 1280×720 VP9),
recorded during this late capture and decoded for visual inspection. The human
HUD shows population 2,725/2,760 and the Imperial Age. Attach it later with
`SHIP_VIDEO`; do not commit the video or any original game assets.
