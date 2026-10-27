# Movement model and research

This is a design for AoeWorld, not a claim that the unpublished AoE2 or DE
source implements every detail below. It is based on the 80ff4af2 playtest and
must be tested against gameplay and tick-time measurements.

## Evidence and confidence

Dave Pottinger's [Coordinated Unit Movement](https://www.gamedeveloper.com/programming/coordinated-unit-movement)
(1999) describes stateful movement, collision detection and prediction. It
explicitly identifies oscillation caused by forgetting previous collision
resolutions in Age of Empires, and proposes retaining resolution plans.
[Implementing Coordinated Movement](https://www.gamedeveloper.com/programming/implementing-coordinated-movement)
discusses group movement, formation movement, priorities, yielding and pushing
units out of a narrow passage. These are the author's design techniques, not a
complete specification of the shipped AoE2 algorithm. High confidence in the
principles; low confidence in any claim of exact parity.

Ensemble engineer Herb Marselas's [performance retrospective](https://www.gamedeveloper.com/programming/profiling-data-analysis-scalability-and-magic-numbers-part-2-using-scalable-features-and-conquering-the-seven-deadly-performance-sins)
states that Age of Kings added long-distance and accurate short-distance
pathing and a unit obstruction manager, and used execution caps. The official
[DE update 185872](https://www.ageofempires.com/news/age-of-empires-ii-definitive-edition-update-185872/)
describes improvements to long- and short-range paths, crowded mines,
lumberjacks, formations, and fishing ships; it also acknowledges overlapping
melee groups. These confirm that tiles are not exclusive unit slots and that
shipping pathing still has edge cases. They do not disclose exact radii,
yield thresholds, spot assignment, or water collision rules.

The local decoded Age of Kings DAT (`VER 5.7`, complete table) in the
owner's tonight worktree exposes two collision axes, distinct from selection
size. Inspection on 2026-10-26 confirms these equal-axis radii:

| Actors | DAT tiles | Simulation subunits (rounded up) |
| --- | --- | --- |
| Villager professions, infantry, archers, Monk | 0.2 | 205 |
| Scout Cavalry, Knight, Light Cavalry | 0.25 | 256 |
| King (Vassal) | 0.3 | 308 |
| Battering Ram | 0.45 | 461 |
| Fishing Ship, Galley variants, other ships, Mangonel | 0.5 | 512 |

The table is `/home/anko/Work/projects/AoeWorld-tonight/local-assets/dat-table/table.json`;
its source data and derived assets remain outside Git and images. These are
verified classic DAT dimensions, not a fresh DE extraction. Simulation rules
encode the rounded integer dimensions, as they already encode DAT combat
values. All supported actors have equal axes, so circles need no ellipse
approximation. Confidence is high in the extracted dimensions and moderate
in treating them as hard circular obstructions: the shipped game's complete
obstruction policy is unpublished.

## Proposed simulation contract

* Static terrain and building footprints govern passability; movable units use
  their collision dimensions, with a spatial index for nearby candidates.
  Same-domain units normally maintain the sum of their radii, with swept
  checks preventing tunnelling. Units already overlapping may move apart;
  movement must not freeze them or forcibly displace stationary workers.
  Land and water passability remain distinct, including ships' own sizes.
* Long-distance routes use the existing bounded static planner. At short
  range, inspect a bounded, deterministically ordered neighbourhood, predict
  obstruction, and choose a small lateral path. Retain a chosen side and the
  blocker long enough to prevent oscillation; reset on a new order or changed
  circumstances. On congestion, deterministic priority and bounded waiting
  allow yielding, then retry or request a budgeted replan. No finite timeout
  can guarantee progress in a physically sealed or permanently full region;
  distinguish that from temporary traffic in tests.
* Work and melee positions lie outside the target footprint and within actual
  work or attack range, accounting for actor size. Choose reachable free
  positions near the closest point of the footprint, distribute competing
  reservations around its perimeter, and reconsider invalidated reservations.
  Work begins only after checking the actual position, not merely an ended
  movement order. Ranged actors stop at a reachable point within their range
  and outside any minimum range; they need not crowd the perimeter.
* A formation follows its leader's route, but members may temporarily relax
  offsets and file through narrow terrain, then recover shape. A single
  blocked member must not permanently halt the leader or lose its order.
  Ship traffic uses the same progress and yielding rules with water terrain
  and ship dimensions, not land-unit assumptions.
* Keep all decisions integer and deterministic, with canonical tie breaks.
  Bound neighbour probes, candidate slots, local search and replanning per
  tick; defer excess work fairly. The implementation shares four local
  refinements and four expensive attack-position searches per tick, including
  command admission. Formations retain two local-search slots while present;
  movement and group rotation prevent a permanently favoured first actor.
  Local refinements retain at most 306 nodes in a 17-by-17 window plus exact
  endpoints/corner exits. Nodes are a quarter tile apart, covering a two-tile
  radius without increasing the node cap. Retained routes store exact world
  positions, including local detours; converting them back into tile centres
  would close physically passable gaps. A distant static waypoint is retained
  behind a nearby free rejoin point, so the local search never tries to reach
  an endpoint outside its bounded window. Repeated refinements cannot grow a
  generated queue beyond 512 points. The long-route scheduler keeps its budget.
  Steering tries four lateral directions, then two retreat directions after
  sustained obstruction. Each swept probe reads at most sixteen one-tile
  collision cells and 32 entries per cell. Normal cells are exact; an overfull
  cell rejects new arrivals and incoming movement. Already compressed spawn
  crowds may escape outwards under bounded correction; this is explicitly a
  transient overlap policy, not a sampled proof of collision freedom. Spawn
  correction may rearrange existing overlaps but sweeps against every initially
  separated body, so repairing one overlap cannot create a new pair.
  Measure p50 and p95 and activity counts on scale, smoke and late-game
  workloads against the same source and machine. Target-hotspot p95 must not
  regress from dev. Shared-host synthetic results alone cannot qualify a
  real match or dedicated hardware.

## Verified problems in the 80ff4af2 base

`movement_step/avoidance.rs` uses a fixed 640-subunit clearance for all units
(1024 subunits per tile); `spacing.rs` applies a separate 768-subunit spacing.
`spacing/destinations.rs` additionally treats any same-domain occupant of an
arrival tile as blocking, even when circles could fit. Gather approaches scan
and reserve whole perimeter tiles, while `gathering/jobs.rs` caps wood at two
and other resources at eight workers per node. Attack search finds a reachable
in-range tile but does not reserve distinct melee positions in that search.
`movement_step/settling.rs` can cancel an obstructed order near its goal;
`gather_step` then accepts a neighbouring tile and proceeds even when its
optional step toward the resource was blocked or never attempted. These are verified code properties, not proof that any one alone caused every
reported stall or the p95 spike. Profile and reproduce each symptom before
attributing a cause.


## Implementation choices and remaining confidence limits

Work samples follow an expanded footprint perimeter, spaced by the actor's
DAT diameter plus a small integer margin. Rank free candidates by distance from the actual
actor position and canonical coordinates. Reservations survive return trips
and use the exact position. Admission derives its broad capacity from the
number of radius-spaced samples; terrain and existing bodies further constrain
it. There is no eight-worker ore cap or whole-tile reservation. Work starts
only outside the resource footprint and within the actor radius plus 104
subunits (256 for a ship's bow). These work reaches and the finite sample
layout are AoeWorld choices, not verified AoE2 constants.

Melee approaches choose free perimeter positions through static reachability;
unit-target candidates use the sum of both collision radii (clamped to the
existing attack reach), while building candidates follow its footprint;
ranged approaches retain the existing attack-range rules. Cheap nearby static
rays are bounded, and costlier searches share the four-search budget. A spent
budget or a temporarily full perimeter defers an attack without dropping its
job. Melee actors already overlapping must separate before hitting.

Formation anchors retain the slowest member speed on cohesive marches and
use half that pace while assembling or stretched, so a delayed rear does not freeze the
front on intermediate slots. The ordinary shared route and column steering
remain active until members recover their shape. Normal/column gaps include
16 subunits beyond the largest land diameter; integer diagonal rotation must
not round two siege bodies into an overlap at their final slots.

Short-range steering remembers its chosen side across ticks and changes side
only after sustained obstruction. Moving traffic cannot cancel an order as
unreachable. A plain move may finish at the closest reachable free position
near its assigned endpoint after sustained obstruction. Its exact source is
also considered when within 1.5 tiles: a legal fractional pocket need not
contain a free search-grid point. Work approaches finish only at actual contact. Idle correction applies only
to actual radius overlaps, including compressed spawns; it no longer enforces
a separate, larger cosmetic spacing. Confidence is moderate in these policies
until the scenario and real-match observations establish their practical limits.
