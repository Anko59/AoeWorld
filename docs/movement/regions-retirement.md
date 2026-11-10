# Region retirement investigation

Base revision: `7f7f68c99ea8`. Measurements below use the deterministic
`a_general_army_engages_idle_defenders` scene, 1,800 ticks, on the shared host.
They do not qualify dedicated hardware or finished RTS performance.

## Engagement regression

The saved retirement patch is not needed to reproduce the failure. An isolated
experiment changed only `check_destination_pocket` to return success without
running its reverse flood. Simulation behavior code, endpoint resolution,
gates, radius buckets, path-cache keys and deadlines were unchanged.

| Observation | Baseline | Pocket work removed |
|---|---:|---:|
| Unit-ticks with an enemy in reach | 85,834 | 77,986 |
| Share with a target | 86.4% | 81.2% |
| Soldiers targetless in reach for over six seconds | 1 | 4 |
| Route failures, disconnected verdicts, expiries | 0 | 0 |
| Total fair route work | 5,400 | 0 |
| Inclusive `plan_serve` time, one diagnostic run | 14.031 ms | 11.951 ms |
| Inclusive `plan_fast` time, same run | 98.757 ms | 65.985 ms |

The one commander is entity **361**, initially controlling 160 soldiers out of
190 spawned soldiers. Sampled positions, tasks, targets and route counters are
identical at every 100-tick checkpoint through tick 1300. At tick 1400 the
baseline has two deferred successful routes; removing the proof answers both
at issue. No changed connectivity verdict is necessary to cause the regression.
The change is route admission timing, followed by a different combat trajectory.

A finer baseline trace identifies the deferred wounded pikemen:

- Entity **153**, tick 1310: `(121,108) -> (89,93)`, 17 HP, `NoAttack`,
  8,192 charged work; the route finishes searching at tick 1311 with 9,780 work.
- Entity **161**, tick 1320: `(120,106) -> (89,93)`, 13 HP, `NoAttack`,
  8,192 charged work.

These are the General's wounded retreats to its castle, not searches across
an enemy gate, bridge or radius-dependent neck. The baseline commander is
still fighting at tick 1700. Without pocket work it is already retreating at
1700 and dead by 1800. The largest extra targetless intervals belong to knight
78 (+164 unit-ticks), spearman 76 (+121), skirmisher 98 (+121), spearman 60
(+118), and archer 61 (+118). These are observations, not evidence that any
one soldier's movement alone caused the commander's death.

Raw diagnostic traces and the isolated experiment are retained outside Git
under `.cache/regions/diagnose-*.log` and `.cache/regions/cause-measured.txt`.
The temporary diagnostic test deliberately failed after printing its data; it
is not part of the committed test suite. The original 85% assertion stays.

## Cold endpoint queries

A regression test puts one tree between two nearby points on a 16,384-tile
map. Before the change it fails on its tenth chunk read: the first detour
starts a complete map build. A second test encloses a destination in a small
wall and bounds the chunks read independently of the surrounding map's area.

Endpoint queries now retain demand-built labels in the region layer and walk
the directed region graph from both endpoints. A sealed destination component
can be exhausted without exhausting its surrounding continent. Frontier
candidates retain the existing distance, origin-distance, y, x ordering.
Changed chunks refresh their signatures on demand; gate access changes do not
relabel unchanged geometry. An explicit complete component census still builds
the union-find graph, consuming the same labels rather than duplicating them.

Validation and any remaining retirement limits are recorded with each commit.
No world-hash repin is justified by a cache-only change.

## Why the surviving army stops engaging

A commander's death removes its soldiers from `generals.controlled` but used to
leave its movement orders alive. `advance_acquisition` permits a moving soldier
to acquire a contact target only while it remains a General's follower. The
released soldiers therefore continue moving and skip acquisition. A regression
with a passive enemy beside a released knight fails on the baseline; cancelling
the commander's orphaned movements makes it pass. Existing attacks continue,
and a separate test preserves a detached player's movement order.

With that fix, the isolated pocket-removal experiment passes the original 85%
engagement assertion and the original six-second idle bound. The cold village
formation case takes 14.119 ms on a 16,384 x 16,384 world, with zero global
union-find rebuilds. These are synthetic local observations, not hardware
qualification.

The broader General gate also exposed a pre-existing failure at the untouched
base: `a_squad_refused_while_sealed_in_marches_once_the_ground_opens` fails its
"sealed in" assertion. A squad cut off from both its commander and its intended
goal now retains its refusal/retry instead of completing a formation around a
local frontier. Ordinary unreachable goals in the commander's reachable ground
still use frontier layouts. The original assertions are retained.

## Gathering crowd regression (#743)

At `b91e9cd17abf`, all three gathering crowd tests fail with their existing
limits. The distant twelve-villager gold case has only nine collectors and
three idle workers after 1,200 ticks; nearby gold's final first load takes
254 ticks against its 141-tick limit.

The disconnected-near-route check introduced in `00d3080c54` queries the
planner's requested destination even after endpoint refinement has certified
a different reachable tile. A diagnostic run records the gold approach from
`(224,89)` to requested `(217,86)` being cancelled at tick zero despite its
certified endpoint `(216,85)`. The corresponding food approaches are cancelled
the same way. Radius buckets, gate admission and active-mover removal do not
cause that incorrect verdict.

The scheduler now checks the certified endpoint when present. A focused
regression pauses a planner immediately after certification and gives the
scheduler zero search work: the old check discards that route. It also closes
the certified endpoint and requires the route to fail, preserving rejection
and mover/accounting cleanup when the actual route becomes disconnected.

Raw logs and before/after crowd measurements remain outside Git under
`.cache/regions/cx-rgcrowd-*`.

| Scenario | Before | After | Unchanged limit |
| --- | --- | --- | --- |
| Distant gold, collectors / idle at 1,200 ticks | 9 / 3 | 12 / 0 | 12 / 0 |
| Distant food, collectors / idle at 1,200 ticks | 8 / 4 | 12 / 0 | 12 / 0 |
| Nearby gold, last first load (ticks) | 254 | 141 | 141 |
| Nearby wood / food, last first load (ticks) | 190 / 179 | 190 / 179 | 190 / 179 |

All three original crowd cases and all three route-region cases pass, including
the new regression and the existing one-work-per-tick sealed-search bound.

Before/after test evidence video: `.cache/showcase/cx-rgcrowd.webm`, generated
by `make showcase SHOWCASE_STORYBOARD=.cache/showcase/cx-rgcrowd.json
SHOWCASE_OUT=.cache/showcase/cx-rgcrowd.webm`. `make video-probe` with that
absolute `SHIP_VIDEO` reports VP9, 1280 x 720, 25 fps, 20.16 seconds. Frames
at 6, 11 and 17 seconds were inspected. This is a labelled presentation of
the measured synthetic tests, not gameplay footage or hardware qualification.
Video SHA256: `732c430901983450b9a46da3772e297f1c160ab89ca6683478c9a19a79da896c`.

The first full simulation run selects all 1,260 tests: 1,259 pass, none are
skipped, and only the previous late-game hash fails. Its independent one/four
worker equality assertion and all original activity floors pass before that
hash comparison. Preserving certified gathering routes changes observable
unit orders and jobs; the current checkpoint is therefore refreshed to
`2a0848c5c015012cd58c02c48a62c0849bca08e16a8db429275857df1aba9730`.
The workload and activity floors are unchanged.

The second full simulation run passes all **1,260 tests**, with zero failures
or skips, including the refreshed checkpoint and its one/four-worker equality
comparison. While it ran, `origin/dev` advanced to `154862c45` (#753). The
branch was rebased, preserving the new five-deployment General gate and the
existing route diagnostics. Only two test files changed across that rebase;
production code is byte-identical to the full-run build. All tests in those
changed modules, plus the crowd and route-region regressions, were rerun on
the rebased source: **16 passed**.

Exact native commands:

```sh
NEXTEST_TEST_THREADS=2 make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'
NEXTEST_TEST_THREADS=2 make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(economy::general::engage::)|test(parallel::)|test(economy::gathering::crowd::)|test(routes::regions::)'
NEXTEST_TEST_THREADS=2 make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-map AOE_NATIVE_TEST_FILTER='all()'
```

The full map rerun passes **402 tests**, with zero failures or skips. Its
first run had 401 passes and one timing failure during concurrent lint:
churn's median flush was 2.112021 ms against the unchanged 2 ms limit
(p99 16.535643 ms). With this session's compiler idle, the complete rerun's
median was 1.649248 ms (p99 3.835441 ms); union-find median was 604.412 us,
p99 1.408712 ms. No threshold, workload or baseline was changed. Both reports
remain in `cx-rgcrowd-churn-during-lint.txt` and `cx-rgcrowd-churn-final.txt`.
These shared-host synthetic observations do not establish hardware performance.

Required local checks pass. Commands used for the static/hook checks:

```sh
make hooks-install hooks-check fmt-check structure-check architecture-check docs-check deny browser-check
make hooks-install hooks-check fmt-check
make lint structure-check architecture-check docs-check
```

Full-run and rebase evidence is in `cx-rgcrowd-simulation-final.log` and
`cx-rgcrowd-rebase-tests.log`; map evidence is in `cx-rgcrowd-map-all.log`
and `cx-rgcrowd-map-final.log`, all under `.cache/regions/`. The full simulation suite was not
repeated a third time after the tests-only rebase; the changed test bodies
are covered by the 16-test run above. No CI, push, real-match performance or
dedicated-hardware qualification is claimed.
