# Gameplay quality metrics

`make quality-metrics` executes seeded scenarios without a browser or wall-clock
inputs. It acquires the shared long-job slot before compilation and measurement,
and refuses below the harness memory floor. This is a measurement harness,
not a movement or combat redesign.
Gameplay acceptance uses the numbers, including regressions, rather than equality
to an old replay hash. Determinism compares repeated **current** runs.

## Measurement contract

Reports store integer counts and fixed-point ratios. `ppm` means parts per million;
`milli` means thousandths. Missing denominators produce JSON `null`, never zero.
One tile is 1024 position subunits. Quantiles use nearest rank, including p95 for
small armies. Only first arrivals contribute to destination-time quantiles;
non-arrivals remain in the arrival denominator and are explicitly censored at the
scenario deadline. A low conditional arrival time cannot offset failed arrivals.

- Arrival is first entry within two tiles of the assigned destination slot.
  Ungrouped marches use their explicit destination. Combat cases measure entry
  within two tiles of the requested objective, never a retargeted tactical slot.
  A dead or unordered actor remains in the original cohort; death never removes a failed arrival.
- Straight-line lower bounds use integer distance and each actor's speed at
  order issue, subtracting the two-tile arrival tolerance and rounding up to
  ticks (at least one). Time ratios compare each arrival to its own
  bound. These ignore terrain and traffic; detours legitimately increase ratios.
- Obstruction samples authoritative collision/recovery state, not an inference
  from zero speed. Mean and worst totals count ticks per originally ordered unit.
  Sampling ends at each member's first arrival. Zero tiles moved means maximum
  displacement from the start stayed below one tile.
- Engagement requires a resolved damaging impact by the actor. Acquiring a target
  or launching a projectile does not count. First contact is the first such hit.
- Damage is effective HP removed in quarter-HP units, capped at remaining HP.
  Damage per unit per fight-minute uses the original cohort size and the entire
  fixed observation window, including approach and time spent dead. Exchange is
  damage dealt divided by damage received; no received damage gives `null`.
- Coherence samples each available intended formation slot, in integer subunits.
  Mean and maximum deviations cover each member until its first arrival. A
  march tick is out of shape if any sampled member deviates by more than one tile. Missing
  slots are counted separately; single-unit marches and Steward transfers have
  no formation. A one-unit attack-move can still expose a formation slot.
  The out-of-shape denominator includes ticks with at least one available slot;
  no shape observations produce `null`, rather than a perfect score.

## Scenarios and commands

The matrix has four march paths and three military combat paths. A Steward
transfers villagers between staffed Town Centers; it has no military attack
order. Single means one ordinary unit, formation means forty units, General
means forty adopted soldiers plus their commander, and Steward means forty
adopted villagers. The General has at least two independently routed squads.
Militia and villagers have different speeds; the lower bound
uses each actor's actual speed. These compare actual order semantics, not equal
CPU cost or identical army composition.

| Scenario | Terrain / action | Deadline (ticks) |
| --- | --- | ---: |
| long | Existing 1,010-tile real-terrain fixture, three river detours | 40,000 |
| quality-march | 575-tile prepared map: ford, two completed bridges, forest walls and one-tile pass | 24,000 |
| source-river | Locked France elevation, water, vegetation and hydrology pages; a completed bridge on the original river | 8,000 |
| attack-move-defended | Equal militia cohorts; defenders also have a Watch Tower | 2,400 |
| melee | Equal militia cohorts on open ground | 2,400 |
| ranged | Equal archer cohorts on open ground | 2,400 |

The controlled fixtures use the real map generator, typed river geometry,
resource obstacles and ordinary navigation. Their page inputs are synthetic;
only `source-river` uses real geographical sources. The France setup retains
its pages and finds a local bridge span and nearby clearings in canonical order.
The ordinary certified village starting kit provides setup resources if the
home clearing lacks them; every order path gets identical starting resources.
Terrain pages stay intact. No save, source page, package, trajectory or recording
belongs in Git. The 100-player war save is not replayed: it is a different workload from controlled
quality cohorts. This does not qualify finished RTS performance, the scale
ladder, or dedicated hardware.

Use an empty frame directory for a new recording: case backgrounds are cached,
and a new run can finish earlier than the previous one. The movie is a diagnostic
top-down projection, not a rendered client or hardware-performance qualification.

```sh
export AOE_COORDINATOR=landscape
make quality-metrics QUALITY_METRICS_ARGS='--label baseline --workers 1'
make quality-metrics QUALITY_METRICS_ARGS='--label repeat --workers 2 --compare reports/quality/REV/baseline-001/report.json'
# Optional diagnostics; frames and movie stay ignored.
make quality-metrics QUALITY_METRICS_ARGS='--label film --frames .cache/quality-video'
make quality-video
```

`--compile-load` starts a real fresh-target Cargo release build in the same
Docker container and shared memory/CPU caps, waiting for a compilation banner.
An unused unique cfg forces compiler work rather than object-cache hits. The
build must remain alive at both ends of the complete matrix, or the run fails.
Its owned process group is stopped and its disposable target removed afterward;
logs stay in `.cache/quality-compile-load`. Idle means no deliberate compiler
load from this command; the shared machine is not exclusive. Reports record
host load averages and compiler liveness separately from deterministic metrics.

`QUALITY_METRICS_ARGS` also accepts `--scenario NAME`, `--package-directory`,
`--source-hash`, and `--out-directory`. A focused selection is explicitly a
partial matrix. Default package hash:
`fdc59853e2404cb5c7fb245f21768970aeb01febe20fd4f15dc96ff522fc9a49`.
The loader validates package identity, source locks, ordered page roots and
page schemas, and fails if local inputs are missing. History schema 1 stores
revision, dirty state, source-content digest, package hash, label, worker count,
measurement definitions and metric values. The source digest hashes source
contents in canonical path order, independently of staging and generated
history. Names are revision-keyed and exclusive-created per run under `reports/quality/REV/LABEL-NNN/report.json`,
so repeated labels never overwrite history and directories stay within the
repository's code/config file-count limit. Values compare
exactly; labels and worker metadata do not enter repeatability equality.
Reports contain aggregate metrics and scenario parameters, not raw unit states.

## Instrumentation cost

Damage recording is opt-in and excluded from snapshots and gameplay hashes.
Disabled: constant-size HP arithmetic and one predictable branch per resolved
impact, no new scan per tick.
Enabled: one small event per impact, drained after each measurement tick.
Obstruction queries reuse existing unit recovery and formation blocked state;
the harness samples the original cohort once per tick. No counter changes
orders, planning budgets, movement, hit resolution or target selection. Focused
simulation tests compare observed and unobserved current gameplay hashes tick
by tick and check summed impacts against actual HP loss, including overkill.
The cost statements describe operations, not measured hardware timings.

## Recorded dev baseline

Gameplay revision: `252bf878ecd76550fbc33f7119c70691eeeb99d5`, the dev snapshot
at task start. This is not a claim about newer `origin/dev` commits. The
measurement-only changes were uncommitted during collection (`dirty: true`);
the source-content digest binds the exact harness and fixtures:

`852b2326a9f73f0ca9918aec874c9105b0b30701662bfe912c5ce64f97cf7458`.

All isolated single-unit, formation and General marches arrived. The expected
isolated-single-unit resilience deficit was **not observed** on these static
routes. Individually ordered Steward transfer cohorts arrived at 80%, 82.5%
and 87.5%, versus 100% for formations. Different unit speeds, controller
semantics and the additional General mean this is not a pure algorithm or
identical-composition combat comparison. No threshold encodes the expected
ranking, and no gameplay was changed to obtain it.

Exact aggregate values: [baseline JSON](../reports/quality/252bf878ecd76550fbc33f7119c70691eeeb99d5/idle-1-001/report.json).

**The combat rows below never armed attack-move.** The `single` and `formation`
cases of this collection passed the flag as the queued-leg argument, so both
cohorts marched ordinarily into the enemy. A marching unit neither acquires nor
retaliates, which is why their engagement and damage read zero: those rows
measure a march, not a fight. The orders are fixed; recollect the matrix before
quoting a combat row.

Tables display ratios, means and tile deviations to three decimals; the JSON keeps exact integer values. Times are simulation ticks, 20 ticks/s. An em dash means no denominator or observation.

| Scenario | Path | Units | Arrived / never | Arrival | Time p50 / p95 | Time / lower bound p50 / p95 | Obstructed mean / worst ticks | Zero tiles | Observed / deadline |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| long | single | 1 | 1 / 0 | 100.000% | 23267 / 23267 | 1.039 / 1.039 | 0.000 / 0 | 0 | 23267 / 40000 |
| long | formation | 40 | 40 / 0 | 100.000% | 25573 / 25582 | 1.144 / 1.146 | 4876.025 / 15007 | 0 | 25586 / 40000 |
| long | general-squads | 40 | 40 / 0 | 100.000% | 23899 / 24235 | 1.062 / 1.084 | 3883.150 / 17519 | 0 | 24236 / 40000 |
| long | steward-transfer | 40 | 32 / 8 | 80.000% | 26371 / 26607 | 1.046 / 1.056 | 288.975 / 615 | 0 | 40000 / 40000 |
| quality-march | single | 1 | 1 / 0 | 100.000% | 14462 / 14462 | 1.136 / 1.136 | 0.000 / 0 | 0 | 14462 / 24000 |
| quality-march | formation | 40 | 40 / 0 | 100.000% | 16145 / 16263 | 1.273 / 1.284 | 5154.475 / 10614 | 0 | 16276 / 24000 |
| quality-march | general-squads | 40 | 40 / 0 | 100.000% | 15166 / 16023 | 1.175 / 1.262 | 3103.125 / 8817 | 0 | 16040 / 24000 |
| quality-march | steward-transfer | 40 | 33 / 7 | 82.500% | 16652 / 22351 | 1.168 / 1.559 | 319.725 / 601 | 0 | 24000 / 24000 |
| source-river | single | 1 | 1 / 0 | 100.000% | 2057 / 2057 | 1.013 / 1.013 | 0.000 / 0 | 0 | 2057 / 8000 |
| source-river | formation | 40 | 40 / 0 | 100.000% | 2237 / 2418 | 1.146 / 1.218 | 908.325 / 1722 | 0 | 2452 / 8000 |
| source-river | general-squads | 40 | 40 / 0 | 100.000% | 2206 / 2302 | 1.140 / 1.174 | 848.725 / 1740 | 0 | 2339 / 8000 |
| source-river | steward-transfer | 40 | 35 / 5 | 87.500% | 2374 / 2783 | 1.054 / 1.284 | 89.425 / 176 | 0 | 8000 / 8000 |
| attack-move-defended | single | 1 | 0 / 1 | 0.000% | — / — | — / — | 15.000 / 15 | 0 | 2400 / 2400 |
| attack-move-defended | formation | 40 | 0 / 40 | 0.000% | — / — | — / — | 631.575 / 1463 | 0 | 2400 / 2400 |
| attack-move-defended | general-squads | 40 | 0 / 40 | 0.000% | — / — | — / — | 413.225 / 1843 | 0 | 2400 / 2400 |
| melee | single | 1 | 1 / 0 | 100.000% | 2395 / 2395 | 1.046 / 1.046 | 20.000 / 20 | 0 | 2400 / 2400 |
| melee | formation | 40 | 2 / 38 | 5.000% | 2343 / 2357 | 1.065 / 1.071 | 702.825 / 1637 | 0 | 2400 / 2400 |
| melee | general-squads | 40 | 0 / 40 | 0.000% | — / — | — / — | 193.725 / 1817 | 0 | 2400 / 2400 |
| ranged | single | 1 | 1 / 0 | 100.000% | 2245 / 2245 | 1.046 / 1.046 | 20.000 / 20 | 0 | 2400 / 2400 |
| ranged | formation | 40 | 17 / 23 | 42.500% | 2243 / 2291 | 1.064 / 1.077 | 510.950 / 1469 | 0 | 2400 / 2400 |
| ranged | general-squads | 40 | 0 / 40 | 0.000% | — / — | — / — | 136.025 / 366 | 0 | 2400 / 2400 |

| Scenario | Path | General squads | Slot deviation mean / worst (tiles) | Out of shape | Missing slot unit-ticks |
| --- | --- | --- | --- | --- | --- |
| long | single | — | — / — | — | 0 |
| long | formation | — | 0.618 / 5.209 | 61.706% | 0 |
| long | general-squads | 2 | 0.944 / 7.916 | 98.609% | 360 |
| long | steward-transfer | — | — / — | — | 0 |
| quality-march | single | — | — / — | — | 0 |
| quality-march | formation | — | 1.166 / 7.400 | 80.093% | 0 |
| quality-march | general-squads | 2 | 1.909 / 10.969 | 99.214% | 360 |
| quality-march | steward-transfer | — | — / — | — | 0 |
| source-river | single | — | — / — | — | 0 |
| source-river | formation | — | 5.039 / 12.393 | 100.000% | 0 |
| source-river | general-squads | 2 | 1.228 / 10.283 | 100.000% | 360 |
| source-river | steward-transfer | — | — / — | — | 0 |
| attack-move-defended | single | — | 0.037 / 0.280 | 0.000% | 0 |
| attack-move-defended | formation | — | 0.644 / 10.894 | 43.958% | 3417 |
| attack-move-defended | general-squads | 2 | 0.710 / 5.589 | 97.459% | 17046 |
| melee | single | — | 0.072 / 0.513 | 0.000% | 0 |
| melee | formation | — | 0.710 / 12.378 | 43.417% | 1538 |
| melee | general-squads | 2 | 0.556 / 4.156 | 99.761% | 18124 |
| ranged | single | — | 0.071 / 0.552 | 0.000% | 0 |
| ranged | formation | — | 0.344 / 4.053 | 22.241% | 9460 |
| ranged | general-squads | 3 | 0.958 / 24.566 | 100.000% | 23384 |

| Scenario | Path | Engaged | First contact min / p50 / p95 | Damage dealt / received (HP) | HP per unit per fight-minute | Exchange |
| --- | --- | --- | --- | --- | --- | --- |
| attack-move-defended | single | 0.000% | — / — / — | 0.000 / 8.000 | 0.000 | 0.000 |
| attack-move-defended | formation | 0.000% | — / — / — | 0.000 / 1068.000 | 0.000 | 0.000 |
| attack-move-defended | general-squads | 100.000% | 1917 / 2007 / 2154 | 1144.000 / 336.000 | 14.300 | 3.405 |
| melee | single | 0.000% | — / — / — | 0.000 / 8.000 | 0.000 | 0.000 |
| melee | formation | 0.000% | — / — / — | 0.000 / 820.000 | 0.000 | 0.000 |
| melee | general-squads | 100.000% | 1917 / 1980 / 2118 | 1286.000 / 448.000 | 16.075 | 2.871 |
| ranged | single | 100.000% | 2305 / 2305 / 2305 | 12.000 / 28.000 | 6.000 | 0.429 |
| ranged | formation | 47.500% | 2313 / 2323 / 2371 | 126.000 / 834.000 | 1.575 | 0.151 |
| ranged | general-squads | 90.000% | 1742 / 1819 / 2201 | 898.000 / 162.000 | 11.225 | 5.543 |

## Repeatability

All six full matrices have identical integer metrics: **zero tolerance** across
1, 2 and 4 simulation workers. Metadata (load, compiler PID, label) is excluded
from equality. The first run is the reference; the other five returned successful
`--compare` checks. Idle denotes no compiler deliberately started by this command,
not an exclusive machine. The three load runs began actual fresh-target release
compilation and required its process to stay alive through the complete matrix.

| Run | Workers | Cases equal | Compiler active start / end | Host load (1m), start / end |
| --- | ---: | --- | --- | --- |
| [idle-1](../reports/quality/252bf878ecd76550fbc33f7119c70691eeeb99d5/idle-1-001/report.json) | 1 | 21 / 21 | none requested | 3.62 / 6.47 |
| [idle-2](../reports/quality/252bf878ecd76550fbc33f7119c70691eeeb99d5/idle-2-001/report.json) | 2 | 21 / 21 | none requested | 5.41 / 6.40 |
| [idle-3](../reports/quality/252bf878ecd76550fbc33f7119c70691eeeb99d5/idle-3-001/report.json) | 4 | 21 / 21 | none requested | 5.55 / 5.75 |
| [compiling-1](../reports/quality/252bf878ecd76550fbc33f7119c70691eeeb99d5/compiling-1-001/report.json) | 1 | 21 / 21 | yes / yes | 6.60 / 13.25 |
| [compiling-2](../reports/quality/252bf878ecd76550fbc33f7119c70691eeeb99d5/compiling-2-001/report.json) | 2 | 21 / 21 | yes / yes | 11.92 / 12.45 |
| [compiling-3](../reports/quality/252bf878ecd76550fbc33f7119c70691eeeb99d5/compiling-3-001/report.json) | 4 | 21 / 21 | yes / yes | 9.41 / 17.21 |

Exact collection commands (executed serially, after memory/load admission):

```sh
export AOE_COORDINATOR=landscape
make quality-metrics QUALITY_METRICS_ARGS='--label idle-1 --workers 1 --frames .cache/quality-video'
make quality-metrics QUALITY_METRICS_ARGS='--label idle-2 --workers 2 --compare reports/quality/252bf878ecd76550fbc33f7119c70691eeeb99d5/idle-1-001/report.json'
make quality-metrics QUALITY_METRICS_ARGS='--label idle-3 --workers 4 --compare reports/quality/252bf878ecd76550fbc33f7119c70691eeeb99d5/idle-1-001/report.json'
make quality-metrics QUALITY_METRICS_ARGS='--label compiling-1 --workers 1 --compile-load --compare reports/quality/252bf878ecd76550fbc33f7119c70691eeeb99d5/idle-1-001/report.json'
make quality-metrics QUALITY_METRICS_ARGS='--label compiling-2 --workers 2 --compile-load --compare reports/quality/252bf878ecd76550fbc33f7119c70691eeeb99d5/idle-1-001/report.json'
make quality-metrics QUALITY_METRICS_ARGS='--label compiling-3 --workers 4 --compile-load --compare reports/quality/252bf878ecd76550fbc33f7119c70691eeeb99d5/idle-1-001/report.json'
```

The narrated diagnostic recording is `.cache/showcase/quality-metrics.webm`
(outside Git); the silent encode is `.cache/quality-video.webm`. Cyan marks
the original cohort, red opponents, yellow objectives, purple slots, blue water,
dark green resource obstacles and brown terrain barriers. The bar counts first arrivals,
including actors that subsequently leave the destination. Frames sample every
100 simulation ticks plus each case's final tick; combat samples are held for
three frames for readability. The encoded movie runs at 24 fps, about 110 seconds.
Captions name every scenario/path and
show final arrival and engagement counts. Playback is accelerated and is not
wall-clock performance evidence.
