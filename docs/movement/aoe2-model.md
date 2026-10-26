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

Pottinger's [performance retrospective](https://www.gamedeveloper.com/programming/profiling-data-analysis-scalability-and-magic-numbers-part-2-using-scalable-features-and-conquering-the-seven-deadly-performance-sins)
states that Age of Kings added long-distance and accurate short-distance
pathing and a unit obstruction manager, and used execution caps. The official
[DE update 185872](https://www.ageofempires.com/news/age-of-empires-ii-definitive-edition-update-185872/)
describes improvements to long- and short-range paths, crowded mines,
lumberjacks, formations, and fishing ships; it also acknowledges overlapping
melee groups. These confirm that tiles are not exclusive unit slots and that
shipping pathing still has edge cases. They do not disclose exact radii,
yield thresholds, spot assignment, or water collision rules.

The DAT parser exposes two `collision_size` axes for each unit, distinct from
selection size. The parser's synthetic test value is not evidence of a real
villager's DAT value. A villager radius near 0.2 tile is a useful initial
hypothesis, not a verified extraction from the owner's game data. A
[modding discussion](https://forums.ageofempires.com/t/some-genie-editor-questions/137606)
reports 0.2 for most infantry and 0.5 for navy, but does not establish that
these fields alone define movement obstruction. Unit and
ship dimensions must come from the actual DAT data where available, converted
at the asset boundary to deterministic fixed-point simulation values; choose
and document a fallback where unavailable. Do not commit game data or derived
images. An ellipse or conservative circular radius is an implementation
choice until verified against gameplay; a unit is not a whole tile.

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
  tick; defer excess work fairly. Initial design budget: at most four local
  steering candidates per mover per tick, each with at most four index cells
  and 32 entries per cell, and at most four expensive attack fallback searches
  globally per tick. The existing route scheduler supplies a separate global
  expansion budget; local retries must also share a fixed global budget and
  resume fairly on later ticks. A sampled neighbourhood is not proof of
  collision freedom in a dense cell: overflow needs a safe, bounded fallback.
  Do not run a full path search per unit per tick. Measure p50 and p95 as well as counts on
  scale, smoke and late-game workloads, and compare the same workload and
  machine against dev. The target-hotspot p95 must not regress from dev;
  synthetic results alone do not qualify real-match performance.

## Existing code observations to verify with scenarios

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
