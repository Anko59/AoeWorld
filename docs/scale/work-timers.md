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
Castle run passed 111 tests; final branch checks and whole-branch measurements
are recorded below.

After the population-credit correction, all seven queue-timer scenarios and
all three long benchmark replay oracles passed, including the unchanged
14,000-unit checkpoint at one/four workers and 2,400 per-tick sleep comparisons.
Commands: the focused simulation filter `test(training::timers::)` and, with
`NEXTEST_TEST_THREADS=2`, `test(benchmark::tests::sleep::)|test(late_game_full_population_replays_at_the_profile_checkpoint)`.

## Walking gatherers

Walking jobs leave the ordered awake set until movement reports contact,
termination or failure. Intermediate movement updates only refresh derived
contact subscriptions. A job already at geometric contact but blocked by a body
subscribes to nearby occupancy changes. Ground and gathering-job changes notify
the same subscribers. Crowded ordinary nodes retain their original near-site
worker-id-modulo-second refinement deadlines.

Deferred contact notifications drain between awake jobs in entity order. Thus a
lower-id job can release a work spot for a later job in the same tick, preserving
the original serial phase. Turning sleep off wakes every walking subscription.
The test-only detector checks that each sleeping walker still satisfies the
original skip predicate.

Five focused scenarios cover distant travel, exact delivery/contact timing,
cancellation, movement termination without a position change, and disabling sleep
mid-trip. The broad gathering/farming/fishing/construction filter passed 177
tests. All three long replay oracles passed with unchanged canonical hashes:
2,400 ticks compared individually, the full late-game sleep pair, and the pinned
14,000-unit checkpoint at one/four workers. Static formatting, lint, structure
and architecture checks passed. A distant returning worker performs fewer than
five awake visits over 160 ticks, versus 160 with sleep disabled.

## Idle military acquisition

An idle fighter can sleep after proving that every spatial bucket its query
touches contains no living hostile identities and that its building search rectangle
contains no hostile structure. Hidden enemies and dense sampled buckets therefore
cannot produce a false empty proof. Such regions wake on unit insertion, removal
or bucket crossings, building placement/removal, source motion/orders/stance,
source vision updates, and research. An unseen enemy already in a touched bucket
keeps acquisition awake, so later sight gains need no separate polling deadline.

Sleeping turns still advance the exact fair cursor and target-sampling round and
charge the original logical targeting budget. Removing those turns would change
which active fighter or tower acquires a target first. `acquisition_checks` counts
actual unit visits; `acquisition_slept` counts turns whose empty query was replayed.
The sleep-off oracle retains the original loop. Test builds verify each empty
proof, source eligibility, spatial membership and charged bucket count.

The base General gate was measured on exact revision
`70d6b53231bfd3f7cf2dd0d191cbf6722bd01d08`. Its five engagement shares were
87.83023054822358%, 89.91759127112012%, 86.36903791038516%,
91.36138757930271%, and 88.45196789682123% (mean 88.786043%).
Command: `NEXTEST_TEST_THREADS=2 make -j1 test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(a_general_army_engages_idle_defenders)'`.
Raw baseline: `.cache/7b-general-before.log` and
`.cache/phase7b-before/.cache/regions/general_vs_idle_variants.txt`.
The acquisition implementation reproduced all five values exactly; `cmp` of the
two generated summaries passed. No engagement threshold or trajectory pin changed.

The broad combat/General/naval/garrison run passed 270 tests. Six new sleep
scenarios cover empty-army visit cost, incoming movement, orders/stances, hostile
buildings, dense friendly buckets alongside an active battle, and the missed-wake
detector. After sharing admission with the ordinary targeting path, those six
and the General gate passed again. Formatting, lint, structure, architecture and
hook checks passed. All three long replay oracles passed before that admission
refactor; the final rerun is retained separately in
`.cache/acquisition-admission-replay.log`.

No Attack, automatic-scouting and embarked identities also sleep through their
unchanged acquisition exclusion. Source/stance changes and ending automatic
scouting wake them. These turns advance the fair cursor without starting a target
probe, exactly as before. Resident enemy corpses cannot become living threats;
they no longer prevent the empty-region proof, while their bucket entries remain
charged to the shared scan budget until removal.

Two added cost replays originally failed with 1,600 disabled-acquisition visits
and 120 corpse-only visits. They now pass bounds below 32 and three respectively,
with unchanged per-tick hashes and correct engagement after the stance event.
The focused acquisition/scouting/General run passed 32 tests; all three long
replays and static checks passed. The five General summaries remain identical.
Logs: `.cache/acquisition-inactive-{red,green,replay,static}.log`.

## Other idle work

Movement's immutable proposal reads job ownership only for its own mover. It no
longer clears or rebuilds a dense bitmap from every gatherer, hunter, attacker
and builder for each proposal batch. The legacy `tables_refresh` population
counter therefore records zero job visits; player-value caching and reusable
proposal/mover buffers remain. The eight-tick late-game cost assertion failed
with 128,000 memberships enumerated before the change and passed with zero.
The buffer-reuse test now checks the remaining player table's actual allocation.

Monks without a healable patient leave the ordered awake set. They subscribe to
the spatial buckets around their healing radius. Combat and wildlife wounds,
wounded unit movement and insertion, source movement/termination, embarkation,
removal, and research wake the appropriate work. Healthy passing workers do not
wake a resting Monk. Moving or embarked Monks wait on source events; the original
healing cadence and entity ordering are unchanged. The sleep-off oracle retains
the original full healer rotation. Test builds reject an empty-region proof
that acquired a patient without a wake, or an unavailable Monk that became ready.

The healthy-crowd test measured 960 bucket entries scanned over 240 ticks before
the change; its sleep-on bound is fewer than 64 with every tick's changes and hash
equal to sleep off. Ten initial tests passed, including genuine damage, wounded
movement within one bucket, healthy motion, the missed-wake detector, embarked
Monk healing suspension/resumption, and movement buffer reuse. Formatting, lint,
structure and architecture checks passed. Broader and final replay runs are
retained in `.cache/rest-focused.log` and `.cache/rest-replay.log`.
The broader run passed 312 tests; all five General engagement results still
match the baseline byte for byte.

Garrison healing now schedules each occupant's existing `heal_at` deadline.
Staggered occupants do not cause a whole shelter scan when only one is due.
Due identities run in the original building/id order; healthy occupants retain
their canonical deadline changes and fractional-HP reset rule. Entry, ejection,
restoration and building mutation refresh affected subscriptions. Destruction
retires them; switching the sleep oracle rebuilds the derived index.

The healthy shelter fixture performs eight healing visits over 300 ticks,
versus 2,400 before, with all hashes and changes identical. Four new scenarios
cover staggered arrivals, wounds, ejection, sleep toggles and a missed-deadline
mutation. The broader garrison/queue/construction/snapshot filter passed 45
tests; all three long replay oracles passed, including the unchanged one/four
worker checkpoint. Formatting, lint, structure and architecture checks passed.
Logs: `.cache/garrison-{red,green,focused,replay,static}.log`.

## Source events and notification cost

Formation rejoin and ordered motion commits wake acquisition and healers when
moving, planning or task eligibility changes without displacement. Two one-unit
phase regressions preserve the sampling clock through these events. The
100-seat, 2,000-tick sleep on/off integration replay passed every tick; its
local diagnostic source and log are `.cache/7b-owner-replay-source.rs` and
`.cache/7b-owner-motion-green.log`. The permanent small regressions live in
`formation/tests.rs` and `movement_step/tests.rs`.

Contact notifications coalesce unchanged subscribers between deliveries, rather
than repeating their ordered-set insertion for every nearby moving body.
Subscription changes invalidate the tile memo. Same-tile endpoints notify once.
Tile and walking-job lookup indexes use fixed hashers; due events and delivered
identities retain ordered sets. The new contact-subscription regression and the
broad queue/construction/walking replay filter passed 90 tests.

## Historical release measurements before the owner rebase

This section, the instruction checks and the video evidence below record the
pre-rebase implementation. The PR #773 follow-up at the end uses the current
branch and its CI dev reference.

Measured on 2026-11-11: before
`70d6b53231bfd3f7cf2dd0d191cbf6722bd01d08`, after
`589888b854f0c081311d11c5f0a106c6fb3356fb`. These are the final simulation
code revisions; later documentation commits do not change that code. The
stop-hook dependency follow-up below changes the lockfile only; these original
reports and recordings retain their measured revision and lockfile provenance.
Both release runs used the identical in-process closed-loop recipe, seed 1,
100 seats, six Rayon workers, 2,000 ticks, scales 1/5/10 and the same index
observer. Whole-window times include startup spikes.

```sh
CARGO_PROFILE_DEV_DEBUG=0 make match-bench MATCH_BENCH_ARGS='--seats 100 --ticks 2000 --threads 6 --scale 1,5,10 --hash-every 100 --out .cache/match-bench/phase7b-before.json'
CARGO_PROFILE_DEV_DEBUG=0 HARNESS_SCCACHE_DIR="$PWD/.cache/7b-cold-sccache" make match-bench MATCH_BENCH_ARGS='--seats 100 --ticks 2000 --threads 6 --scale 1,5,10 --hash-every 100 --out .cache/match-bench/phase7b-after.json'
```

The before command ran in the owned detached base worktree
`.cache/phase7b-before`; its report was copied unchanged to the path below.
The empty local `HARNESS_SCCACHE_DIR` disables the optional compiler cache.
Container resource caps, workload parameters and baselines were unchanged.
Raw reports: `.cache/match-bench/phase7b-{before,after}.json`; logs:
`.cache/7b-before-match-bench-attempt3.log` and
`.cache/7b-after-match-bench-final.log`.

Every run's tick-zero and tick-2,000 population samples agree between revisions:

| Scale | Units: start → end (both runs) | Movers: start → end (both runs) |
|---|---|---|
| 1x | 9,719 → 10,175 | 2,661 → 44 |
| 5x | 46,791 → 46,988 | 13,819 → 592 |
| 10x | 93,080 → 93,277 | 27,744 → 1,593 |

| Scale | Before tick mean / p50 / p95, ms | After tick mean / p50 / p95, ms |
|---|---|---|
| 1x | 50.832 / 22.316 / 38.762 | 43.292 / 19.413 / 32.436 |
| 5x | 151.275 / 81.394 / 426.262 | 148.124 / 85.918 / 362.072 |
| 10x | 329.976 / 176.037 / 807.324 | 279.632 / 184.216 / 565.309 |

Phase p50 milliseconds, before → after; phase percentiles are not additive:

| Scale | Movement | Gathering | Construction/gates | Research/training | Combat |
|---|---|---|---|---|---|
| 1x | 19.367 → 16.877 | 0.179 → 0.165 | 0.021 → 0.060 | 0.078 → 0.003 | 0.880 → 0.493 |
| 5x | 74.131 → 75.393 | 1.044 → 1.138 | 0.002 → 0.003 | 0.059 → 0.003 | 0.786 → 1.073 |
| 10x | 162.802 → 154.628 | 2.940 → 3.259 | 0.002 → 0.003 | 0.071 → 0.003 | 0.786 → 1.140 |

| Scale | Job memberships enumerated by tables_refresh, before → after | Actual gather steps, before / after |
|---|---|---|
| 1x | 1,003,007 → 0 | 29,726 / 29,726 |
| 5x | 10,105,564 → 0 | 114,371 / 114,371 |
| 10x | 36,410,915 → 0 | 199,350 / 199,350 |

Gatherer `visits` and `slept` include logical skipped turns, including sleeping
walkers; they are not physical awake-job visit counts. The unchanged gather-step
counts and canonical hashes establish that activity did not disappear.

All 21 saved checkpoints per scale (63 total) matched. Per-scale comparison
inputs select unchanged run objects from the complete reports and record their
source JSON SHA-256. All three Dockerized comparisons passed:

```sh
make match-bench MATCH_BENCH_ARGS='--compare .cache/match-bench/phase7b-before-1x.json,.cache/match-bench/phase7b-after-1x.json'
make match-bench MATCH_BENCH_ARGS='--compare .cache/match-bench/phase7b-before-5x.json,.cache/match-bench/phase7b-after-5x.json'
make match-bench MATCH_BENCH_ARGS='--compare .cache/match-bench/phase7b-before-10x.json,.cache/match-bench/phase7b-after-10x.json'
```

Comparison logs: `.cache/7b-match-compare-final-{1,5,10}x.log`.
Load averages at report completion (1/5/15 minutes) were
5.85 / 9.96 / 12.10 before and
5.91 / 6.04 / 6.40 after. The shared host was unpinned;
other checks and captures also ran during these windows. These timings do not
establish a controlled wall-time improvement or a quiet-machine 20 Hz result.

## Instruction gate and final checks

The generated 100-seat late-game kernel counts one authoritative tick after its
384-tick warm-up, with one worker and the unchanged recipe/checkpoint.

```sh
NEXTEST_TEST_THREADS=2 CARGO_PROFILE_DEV_DEBUG=0 make -j1 perf-late-game-instructions
NEXTEST_TEST_THREADS=2 HARNESS_SCCACHE_DIR="$PWD/.cache/7b-cold-sccache" make -j1 perf-late-game-instructions
```

The first command ran on the pinned base; the second ran on the source committed
as `589888b854f0c081311d11c5f0a106c6fb3356fb`. Source file SHA-256 checks
passed after the accepted measurement. The cap remains 1% above that base.

| Before instructions | After instructions | Change | Maximum allowed | Result |
|---|---|---|---|---|
| 210,225,349 | 212,044,982 | +0.865563% | 212,327,602 | pass |

Accepted raw NDJSON: `.cache/phase7b-perf/before-instructions.ndjson` and
`.cache/phase7b-perf/after-final-instructions.ndjson`; complete Callgrind traces
are retained beside them. Logs: `.cache/7b-before-instructions.log` and
`.cache/7b-events-final-instructions.log`. A local unsupported Valgrind-client
artifact was quarantined and rebuilt before the accepted measurement; no gate
or performance baseline was changed.

Final implementation validation passed:

- `make hooks-install hooks-check fmt-check lint structure-check architecture-check`.
- `NEXTEST_TEST_THREADS=2 make -j1 test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'`: all 1,310 passed; `.cache/7b-simulation-full-events.log`.
- Focused source-event/acquisition/General checks: 59 passed; final walking/queue/General checks: 20 passed. The five General summaries still match the base byte for byte.
- Workload sanity retains the four-training-site, two-research-site and spawn-inspection floors on the full-step reference, and compares changes and hashes every warm-up tick. Sleeping therefore cannot pass by removing the workload.

The initial `NEXTEST_TEST_THREADS=2 make -j1 preflight` failed at `deny`
(Make exit 2) on documentation revision
`1c4e5e9cc7d7be6fddb733b8bc28267e84a71bfe`: its base-identical lockfile
contained `rustix 1.1.5`, triggering `RUSTSEC-2026-0486`. Original output and
advisory details remain in `.cache/7b-preflight.log` and
`.cache/7b-preflight-deny-process.log`. Two independent five-crate native runs
then reached the existing scoped 600-second deadline; the standalone release
smoke passed. Those attempts remain recorded in `.cache/7b-body.md`.

The stop hook subsequently requested correction of the advisory. Dockerized
`make cargo-pin CARGO_PIN_PACKAGE=rustix CARGO_PIN_VERSION=1.1.6` changed only
the transitive version and checksum. `make deny` passed without any exception;
logs are `.cache/7b-rustix-{pin,deny}.log`. Simulation source, replay pins,
recipes and gate code are unchanged by this follow-up.

`NEXTEST_TEST_THREADS=4 CARGO_PROFILE_DEV_DEBUG=0 make -j1 preflight` selects
all 12 crates because the lockfile changed. Its native suite passed all 4,377
selected tests (20 skipped under the existing policy), followed by passing
doctests. The full-workspace path uses its existing 1,200-second budget and
two-job compiler cap; scoped runs use 600 seconds. Four test threads replace
the earlier conservative two-thread setting. The complete aggregate outcome is
recorded in `.cache/7b-body.md`; output is `.cache/7b-rustix-preflight.log`.

## Local video evidence

`SHIP_VIDEO=.cache/showcase/phase7b-before-after-vp9.webm` is the paired
48-second reel: VP9, 1280×720, 25 encoded fps, no audio. Individual baseline
clips are `7b-<scene>-before-vp9.webm`; final implementation clips are
`7b-<scene>-after-vp9.webm`, all under `.cache/showcase/`.

| Scene | Individual clip duration per build |
|---|---|
| construction | 8 s |
| walking | 3 s, derived from the completed-site worker's normal trip |
| queues | 12 s |
| research | 6 s |
| idle-prepared | 6 s |
| garrison | 8 s |

The before server reported the pinned base; the after server reported
`589888b854f0c081311d11c5f0a106c6fb3356fb` in `/health`
(`.cache/7b-after-health.json`). Both used the same local game assets and real
client with software WebGPU. The takes use ordinary UI actions after scene
reset/activation. The prepared idle/garrison map is local fallback data, not
source-geography qualification. Inspected frames show foundation/completion,
walking, Villager training, Loom progress, the healthy idle Scout and `1 / 15`
garrison occupancy. Failed readiness takes are excluded; no readiness floor
was reduced. Both owned servers were stopped after recording.

All twelve individual clips and the final reel passed `make video-probe` with
`SHIP_VIDEO` set to the relevant path. Final metadata and checksum are
`.cache/7b-combined-metadata.json` and `.cache/7b-combined.sha256`.
Original assets and recordings remain outside Git and container images.

These synthetic workloads and functional captures do not demonstrate finished
RTS performance, sustained real-save load or dedicated-hardware qualification.
No CI run, full e2e suite or publication was performed by this local task.

## PR #773: small-workload timer overhead

This follow-up starts at `c8e14a2178d158478a3c3d771279886bfffad16a`, after
the owner's rebase. Earlier measurement sections describe the pre-rebase implementation, not this PR revision.
The comparison is the [CI report for PR #773](https://github.com/Anko59/AoeWorld/pull/773#issuecomment-6799035895):
PR run `43038321257`, dev run `43029364718` at
`b7c6db5daa9bbb577f1634a135420b760e41c26e`. The owner's requested small-row
limit is +1% of that dev run; no checked-in baseline or gate changed.

Local measurements use the unchanged single-worker Callgrind kernels:
`simulation::smoke_ticks` (8,000 units, four ticks),
`simulation::distributed_ticks` (128,000 units, four ticks), and
`simulation::late_game` (one tick after its 384-tick warm-up).

| Kernel | CI dev | Local start | Dispatch fix | Combat fix | Final | Final vs dev |
|---|---|---|---|---|---|---|
| smoke_ticks | 5,121,831 | 5,450,635 | 5,309,599 | 5,178,706 | 5,132,786 | +0.214% |
| distributed_ticks | 9,581,069 | 10,293,231 | 9,741,117 | 9,625,834 | 9,625,682 | +0.466% |
| late_game | 210,877,587 | 187,732,602 | 186,172,783 | 184,566,363 | 184,524,496 | −12.497% |

The small-row maxima are 5,173,049 and 9,676,879 instructions respectively.
The local initial smoke row differs from CI by eight instructions; the local
late-game row also differs, so the table preserves both provenances.

The first fix (`4a6d9ed26`) returns when construction/queue tile subscriptions
are absent and avoids gathering event lookups when there are no gathering jobs.
Deferred notifications still drain, and test-only missed-wake checks still run.
Distributed's construction, queue and gathering dispatch cost falls by 193,684,
197,128 and 161,302 instructions: their sum, 552,114, exactly accounts for the
row improvement. Two empty BTreeSet iterator advances per absent subscription
had dominated the first two functions.

The second fix (`9a4c66263`) inlines empty combat wake guards and carries one
transient fact from target acquisition: whether its existing bounded sample saw a living enemy.
Such an enemy already rules out a quiet region, including hidden/out-of-range
enemies; there is no reason to repeat the full proof. An absent sampled enemy
still requires the original proof over unsampled dense entries and buildings.
Target ordering, sample rotation and inspection budgets are unchanged.

The final fix skips the building-target fallback when the building table is
empty. This is an existing scan adjacent to the timer proof, not an additional
timer regression. It still runs the original unit sample and budget updates
first. Compared with the combat fix it saves another 45,920 smoke instructions,
152 distributed instructions and 41,867 late-game instructions. Both small
rows now meet the requested +1% dev limit, while late game retains its gain.

The added single-worker population oracle (`8a274a099`) uses the same seeded
populations and movement orders as the small kernels. Sleep on/off worlds agree
on changed units, canonical hashes and combat work at every tick. Diagnostic
registry inspection on the first fix found no parked or sleeping acquisition/healing jobs in
these four-tick fixtures. Three targeted contracts additionally cover a living
out-of-range enemy, a retained corpse and an unsampled dense-bucket enemy.

Measurements: `NEXTEST_TEST_THREADS=4 CARGO_PROFILE_DEV_DEBUG=0
HARNESS_SCCACHE_DIR="$PWD/.cache/7b-cold-sccache" make -j1 perf-instructions`.
Before, first-fix and final NDJSON and raw traces are retained under
`.cache/773-perf/` with `before-`, `empty-after-`, `combat-after-` and
`empty-targets-after-` prefixes.
Logs are `.cache/773-before-instructions-retry.log`,
`.cache/773-empty-instructions-monitored.log` and
`.cache/773-combat-instructions.log` plus the final
`.cache/773-empty-targets-perf-instructions.log`. Source SHA-256 snapshots
identify the measured tree before committing it. No replay pin or benchmark recipe changed.

Validation: all 1,332 simulation tests passed, zero skipped, including unchanged
General checkpoints and long sleep oracles; focused contracts and all required
static checks passed. Final simulation output is
`.cache/773-empty-targets-simulation-full.log`; the focused and static logs use
`.cache/773-empty-targets-{focused,static}.log`. The final commit's aggregate
preflight result is recorded separately in `.cache/773-body.md`, with complete
output at `.cache/773-final-preflight.log`. No CI run or push is claimed here.
These synthetic instruction counts do not qualify dedicated hardware or
finished RTS performance. The shared-host phase times are diagnostic only.
