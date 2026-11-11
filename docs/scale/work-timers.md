# Economy work timers (movement rework phase 7b)

## Construction

Builder jobs maintain an ordered awake set and foundation membership. A settled
crew shares a constant-rate interval: `builders + 2` integer work per tick.
Only wake events revisit the crew; site deadlines reuse its unchanged count.
Work and earned HP are projected from the interval's start tick when queried, hashed, remembered
through fog or saved. The phase clock denotes the last completed construction
phase, so combat before construction still reads the previous tick's progress.
Building queries return `Cow<GameBuilding>`: unchanged records borrow storage;
records with skipped progress return their exact projected value.
Visible site snapshots still publish progress every tick, as required by the
existing building-memory change stream. That publication visits sites, not
their sleeping crews. A tile-count index preserves sleeping work reservations
without enumerating workers when another builder arrives.

Job insertion, removal and queue edits wake the affected foundation. Damage,
cancellation, removal and building technologies materialize its clock before
changing its inputs. Unit movement, death and removal notify nearby foundations.
A walking builder subscribes to movement's changed-unit events until perimeter
contact or order termination. Settled builders remain reserved at their work
spots while asleep. Blocked approaches retain bounded route retries. Foundation
clearance runs site events that preserve the ordinary displacement work without
polling the crew. Completed farms waiting for a free staffing position sleep
until their original worker-id-modulo-second retry tick.

Repair bills affect resource stocks on each tick. A repair therefore runs one
site event per tick, in building-id order, while its stable crew sleeps. This
preserves fractional resource bills, insufficient-stock stalls and the original
release tick without visiting each repairer. Completed bridges whose deck cannot
yet be installed retain the ordinary retry path.

Test builds check every sleeping worker against the state its skipped step
reads, including foundation blockers. An unannounced input mutation panics with
`missed wake`. Production builds omit these scans. `set_work_sleep(false)` is
the full-step oracle for construction as well as gathering.

Construction validation (Dockerized Make):

- `make hooks-install hooks-check`: passed.
- `make fmt-check`, `make lint`, `make structure-check`: passed.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(construction::)|test(farming)|test(facing::)'`: 85 passed.
- Six timer scenarios compare intermediate observations, snapshots, cancellation,
  crew changes, repair resource bills, walking, and blocked farm staffing; the
  detector also rejects an unannounced damage mutation. The existing 9,000-tick
  farm-reseed oracle passed in the same focused run. No replay hash was repinned.

## Training and research

Queues keep their canonical entries and derived completion deadlines. Only awake
sites and due deadlines run queue logic. Reads, hashes, memory publication and
snapshots project the exact intervening head-entry progress. Research and training
retain separate phase clocks: research effects still precede this tick's training.

Queue edits materialize progress before mutation. Population and housing events
wake the affected player's indexed training sites. A blocked completed queue
subscribes to its exit tiles; movement, removal, embarkation and ground changes
wake it. Cancellation and building loss retire deadlines and subscriptions.
Recoverable environment-page errors retain the ordinary retry path, since those
pages can recover without a ground-change event.

Population notifications wake clocks only when available space changes. Waking
a later-id running queue on a spawn that still leaves space would remove its
clock after the awake snapshot and lose one tick of progress; a focused replay
pins this case. Within training, a population event credits lower-id sites through the current
tick and higher-id sites through the preceding tick. This preserves the serial
rule when one Town Center fills the last housing space while another trains.
Technology effects materialize affected queues before changing their inputs.

The shared sleep-off oracle also disables queue sleeping. Tests compare hashes,
observations and saved queues every tick through cancellation, restart, housing
loss, death, blocked exits and research completion. A raw progress edit is rejected
by the test-only missed-wake detector. The initial focused training/research/
Castle run passed 111 tests; final commit checks and whole-branch measurements
are recorded below when complete.

After the population-credit correction, all seven queue-timer scenarios and
all three long benchmark replay oracles passed, including the unchanged
14,000-unit checkpoint at one/four workers and 2,400 per-tick sleep comparisons.
Commands: the focused simulation filter `test(training::timers::)` and, with
`NEXTEST_TEST_THREADS=2`, `test(benchmark::tests::sleep::)|test(late_game_full_population_replays_at_the_profile_checkpoint)`.

## Local video evidence

The construction baseline is `.cache/showcase/7b-construction-before-vp9.webm`:
eight seconds, VP9, 1280x720, base `70d6b53231bfd3f7cf2dd0d191cbf6722bd01d08`.
The inspected frames show the foundation, completion, and the worker walking
to its next resource. The corresponding after take and other work scenes are
pending. Videos and licensed assets remain outside Git.

## Historical recovery measurements

The following measurements came from the interrupted implementation on
2026-11-09. They are retained as historical evidence only: they do not compare
this branch against its current base, `70d6b53231bfd3f7cf2dd0d191cbf6722bd01d08`.
The required current-base release and instruction comparisons remain pending.

The initial production baseline is `8ca8d40d656626dbb4f1572ed0300093791c9610`.
The baseline executable was built before linking the timer modules and retained
locally as `.cache/timers-before-harness`. The in-process closed-loop run used
16 seats, seed 1, one worker, 600 ticks, scales 1x and 5x, and hashes every 100
ticks. These are whole-window samples, including startup spikes; they are not
the steady-state ticks 301–600 used in older gathering measurements.

| Baseline phase, ms | 1x mean / p50 / p95 | 5x mean / p50 / p95 |
|---|---|---|
| construction and gates | 0.111 / 0.111 / 0.142 | 0.412 / 0.366 / 0.447 |
| gathering and economy | 1.517 / 0.198 / 0.326 | 5.695 / 0.969 / 14.672 |
| research and training | 0.017 / 0.013 / 0.017 | 0.047 / 0.015 / 0.019 |

The run ended with 2,815 / 14,021 units. Tick p50 was 3.3 / 18.6 ms.
Gathering recorded 58,137 / 1,424,420 logical visits, including 11,450 / 130,346
sleeping visits and 44,456 / 1,279,348 walking visits. Construction work counters
were added with the timer implementation; the full-step oracle supplies its
comparison counts.

Raw baseline report: `.cache/match-bench/timers-before.json`; log:
`.cache/timers-bench-before.log`. Measurements use a shared machine (baseline
one-minute load 33.43), with no CPU pinning or dedicated-hardware qualification.
Synthetic in-process work does not demonstrate finished RTS performance.

```sh
make match-bench MATCH_BENCH_ARGS='--seats 16 --ticks 600 --threads 1 --scale 1,5 --hash-every 100 --out .cache/match-bench/timers-before.json'
```
