# Real France country qualification

The observations below were made on candidates generated before the single map
version. The strict reader rejects those packages; regenerate both candidates
before rerunning `make map-country-probe` or `make test-country-source`, and
expect new content hashes. The commands and evidence shape are unchanged.

## Immutable candidates

- Overview-only: `c3436bbd6fd154971bbf79ec6f3b88669123e09d2df41e534fa718f83793e8e6`, seven pinned inputs.
- Explicit vectors: `146c49268ecac72372db928436976fcc7ac96a4350d81dbd3a75b983d69c3b50`, nine pinned inputs, typed water/evidence1024 and categorical water 128.
- Both immutable page/root verifications passed. Original published France baseline remains unchanged.
- Request: circa 600, 1200km geographic side, 30:1, 20000 native tiles; ordinary start `(9999,9999)`.

## Fixed ordinary route observations

The four original endpoints remain E/S/W/N at 128 tiles (256m game distance).
Search remains the ordinary 64chunk start and unchanged 4096 expansion planner.
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
crossability, capped at 4096 visited tiles and 32768 neighbour checks. It does not
increase planner/start budgets, select replacement goals, alter the canonical
50k/1:1 source qualification, simulate movement or activate a live server.
Truncated enumeration cannot certify a complete component or a planner route.
Both actual candidates reached 4096 tiles and truncated: typed bounds
`(9964,9964)..(10033,10033)`,31171 checks; overview bounds
`(9964,9964)..(10034,10034)`,30920 checks. This demonstrates substantial local
connected land, not completeness or global-country playability.

## Separate native simulation orders

Four additional predeclared E/S/W/N32tile endpoints (64m game distance) run
on fresh authoritative GameWorlds with the same source provider. Every result
is retained, no successful endpoint is chosen adaptively and original 256m
results remain unchanged. Each order is capped at 2048 simulation ticks; default
movement/path budgets remain untouched, with no resource depletion.
Typed candidate S/W arrived in 427 ticks each; E/N were rejected.
Overview E/S/N arrived in 479/427/427 ticks; W was rejected.
These are actual native source-backed local moves, not live WebSocket/controller
activation, long-distance travel, measured realtime 20Hz or hardware qualification.
Dockerized focused paired `map-country-probe` gates passed with all outcomes
retained. `hooks-install hooks-check preflight` passed:1222 native tests,
1 existing skip,229.883s test runtime. Synthetic regression tests remain separate
from the actual immutable-provider evidence recorded above.

## Original-endpoint native executor

The observer additionally retains all four original 128tile endpoints in fresh
GameWorlds, with 8192 simulation ticks per order. Existing 2048tick32tile cases,
ordinary 64chunk starts and direct 4096expansion results remain unchanged.
The actual typed candidate reached its original west 256m destination in 1925
simulation ticks; E/S/N were rejected. The overview candidate E/W/N arrived
in 1751/1807/1780 ticks; S was rejected. All sixteen paired native observations
were retained. Thus the raw direct planner's west
BudgetExceeded is not an actual gameplay-executor failure or proof of no route.
No resources were depleted, no endpoint moved and no planner budget raised.

## Isolated actual-source browser capture

`make GID=117 test-country-source` explicitly accepts package directory/hash
through `AOE_SOURCE_QUAL_PACKAGE_DIRECTORY`/`AOE_SOURCE_QUAL_CONTENT_HASH`, plus
an original `AOE_ASSET_PACK` directory. It is not the fixed 11 visual matrix,
the canonical 50k source qualifier or the procedural ordinary E2E fixture.
The runner verifies the original asset pack, stages only selected immutable
manifest/pages into an owned temporary directory (no mutable resource overlays),
starts a disposable server on its own port and activates that exact saved map.
The original private package directory and default server/map remain unchanged.
Each attempt retains a unique ignored run directory, actual native outcomes,
input hash/revision/dirty status, original pack manifest and browser observations.

The latest local capture used implementation revision `b77027fd2ff5854070c901973e59b54fff7bdafc` with `dirty=true`; it is not a claim of clean committed-source, CI or hardware qualification. Explicit WebGPU, forced WebGL2 and forced Canvas identities passed, all serving original pack `7e6fa0da194d13fcff0cd50e74fe92215fd11b5bcce4556e4dad3d34ff7447ce`. Normal and wide 1280x720 source screenshots show the ordinary start glade and neighboring composed forest clusters. Actual hash-prefixed chunk responses, nontrivial source pixels, no page errors and residency<=512 were checked.

The same disposable candidate run decoded the real binary protocol-v8 WebSocket stream. A browser controller issued one right-click order to source tile `(10000,9999)`; evidence retained the matching map hash, Controller role, one sequence-1 order, accepted acknowledgement, observed motion and idle arrival at the exact tile-center destination (19 authoritative position observations). This qualifies one short local live move only, not 20Hz timing, long-distance travel or army movement.

A separate fixed 24-drag local sample observed 93 unique source chunks, maximum resident cache 109/512 chunks, summed Chromium VmRSS peak 890,560,512 bytes, final-four-sample spread 9,584,640 bytes, and disposable server VmRSS peak 29,487,104 bytes. The 512-chunk eviction threshold was **not exercised** (`eviction_limit_exercised=false`); this 20k-tile/30:1 candidate and route do not substitute for canonical 50k/1:1 memory qualification. An exploratory 30,000-pixel sweep produced a map-chunk fetch error and is not counted as a pass. Typecheck, lint and browser formatting passed. Every screenshot still represents only the start region, not every water/coast/relief location.

France is immutable real-source test data only: generator code remains location-agnostic, no map was hand-built, and no France-specific generation branch or default activation was added.

## Limits

The inspected local lines do not justify changing water-model semantics.
A code audit separately identified coarse categorical hydrology amplification,
NODATA shore-estimator assumptions and worst-case vector predicate costs; those
are future representation/measurement risks, not observed causes of these lines.
Real-source local native and one live controller move plus three-backend
start/forest captures are observed above. Longer/global traversal, army movement,
broader water/coast/relief visuals, the canonical 512-chunk eviction/memory gate
and dedicated hardware remain unqualified. Candidate activation occurred only in
the disposable qualification server; default France activation remains unchanged.
