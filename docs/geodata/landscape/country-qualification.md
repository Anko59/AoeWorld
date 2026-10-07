# Real France country qualification

## Immutable candidates

- Overview-only: `c3436bbd6fd154971bbf79ec6f3b88669123e09d2df41e534fa718f83793e8e6`, seven pinned inputs.
- Explicit vectors: `146c49268ecac72372db928436976fcc7ac96a4350d81dbd3a75b983d69c3b50`, nine pinned inputs, typed water/evidence1024 and categorical water128.
- Both immutable page/root verifications passed. Original published France baseline remains unchanged.
- Request: circa600, 1200km geographic side, 30:1, 20000 native tiles; ordinary start `(9999,9999)`.

## Fixed ordinary route observations

The four original endpoints remain E/S/W/N at 128 tiles (256m game distance).
Search remains the ordinary64chunk start and unchanged4096 expansion planner.
No terrain or object was cleared; no easier endpoint substituted.

The typed candidate still reports E/S/N invalid destination and W budget exceeded.
Its 512 straight-line samples have zero water, zero nonwalkable surfaces,
zero physically impassable tiles and zero blocked physical edges.
Effective impassable/resource-present counts are respectively E34, S51, W17, N35.
The E endpoint is a wood/tree node on source-derived dry temperate plateau,
with canopy/floor1000, no clearing reservation and typed NoEvidence/None.
This is actual resource obstruction, not observed river inflation at that tile.

Straight-line observations do not establish planner root cause: bounded search
may explore other terrain, and BudgetExceeded does not prove no route exists.
Likewise, a fixed destination inside a tree is not a valid movement destination;
these four failures alone do not prove the complete country unplayable.
Geography hashing binds all source locks/environment; adding vector inputs also
changes the deterministic forest key despite unchanged overview pyramids.

## Bounded connected-land observation

A separate local component observation follows authoritative passability and
crossability, capped at4096 visited tiles and32768 neighbour checks. It does not
increase planner/start budgets, select replacement goals, alter the canonical
50k/1:1 source qualification, simulate movement or activate a live server.
Truncated enumeration cannot certify a complete component or a planner route.
Both actual candidates reached4096 tiles and truncated: typed bounds
`(9964,9964)..(10033,10033)`,31171 checks; overview bounds
`(9964,9964)..(10034,10034)`,30920 checks. This demonstrates substantial local
connected land, not completeness or global-country playability.

## Separate native simulation orders

Four additional predeclared E/S/W/N32tile endpoints (64m game distance) run
on fresh authoritative GameWorlds with the same source provider. Every result
is retained, no successful endpoint is chosen adaptively and original256m
results remain unchanged. Each order is capped at2048 simulation ticks; default
movement/path budgets remain untouched, with no resource depletion.
Typed candidate S/W arrived in427 ticks each; E/N were rejected.
Overview E/S/N arrived in479/427/427 ticks; W was rejected.
These are actual native source-backed local moves, not live WebSocket/controller
activation, long-distance travel, measured realtime20Hz or hardware qualification.
Dockerized focused paired `map-country-probe` gates passed with all outcomes
retained. `hooks-install hooks-check preflight` passed:1222 native tests,
1 existing skip,229.883s test runtime. Synthetic regression tests remain separate
from the actual immutable-provider evidence recorded above.

## Limits

The inspected local lines do not justify changing water-model semantics.
A code audit separately identified coarse categorical hydrology amplification,
NODATA shore-estimator assumptions and worst-case vector predicate costs; those
are future representation/measurement risks, not observed causes of these lines.
Real-source local native movement is observed above. Longer/global traversal,
source-backed visuals on all three renderers, live controller movement,
memory pressure and dedicated hardware remain unqualified.
No live France activation has been performed by this qualification work.
