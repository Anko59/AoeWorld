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
