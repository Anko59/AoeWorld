# Squads brief: reproduction and delivery evidence

Work started 2026-11-03 02:28 UTC, branch `movement/squads`, with an
unfinished merge at `d2822c783406c739594aa2a3374147944a17bc40`.
The conflict-resolved merge is `2522ae0b1f9fb37582da00e8419731c881e82569`.
It preserves General-only follower yielding and single-file bridge recovery.
The combined baseline's independent one/four-worker replay agrees exactly on
`575a774847525864239b77dcd34908aae5fe73b093b3c239e39061b06325ac11`.

The verified increment contains squad simulation, movement recovery, bridge
components, protocol capacity and their tests. Recording adapters, the recording
constructor and basic HUD wording remain separate working-tree changes pending
browser proof; descriptions of those below are preparation status, not delivered
features of this increment.

## Reproduction scope

The existing `real_terrain` fixtures contain typed hydrology, forest resources
and buildings, using synthetic source pages. They are not real-source map
qualification. The crowded tests leave forty soldiers parked at the forest
exit, add a compact commanded column and thirty stationary villagers. The
General's existing hundred-soldier fixture receives the same thirty villagers.
Observation includes order admission, ending at exact arrival or 12,000 ticks.
At 20 Hz, 60 ticks is three seconds.

Target diagnostics sample attached formation steering every ten ticks.
Blocked counts distinct actors whose target cannot hold their physical body.
Straight counts distinct actors targeting their final slot through obstacles.
Bad run is the longest consecutive sampled interval with either condition.
Detached routes are excluded from these target counts; arrival, speed and
stalls include every commanded actor. Diagnostic geometry has separate work
accounting. No collision radius, arrival threshold or stall threshold changed.

## Before/after measurements

Base is the merged source before the production changes. Current observations
come from `reports/squads/attack-reachability.log`, with exact diagnostics
retained through a local Nextest success-output configuration.

| Crowded case | Version | Arrived | Blocked | Straight | Bad run ticks | Max still ticks |
| --- | --- | --- | --- | --- | --- | --- |
| 16, no General | Base | 16/16 | 0 | 1 | 60 | 1342 |
| 16, no General | Current | 16/16 | 0 | 0 | 0 | 31 |
| 100, no General | Base | 100/100 | 13 | 9 | 460 | 2638 |
| 100, no General | Current | 100/100 | 8 | 0 | 10 | 32 |
| 200, no General | Base | 200/200 | 108 | 115 | 980 | 2883 |
| 200, no General | Current | 200/200 | 0 | 0 | 0 | 50 |
| General + 100 | Base | 100/101 | 9 | 7 | 1490 | 1430 |
| General + 100 | Current | 101/101 | 9 | 0 | 40 | 32 |

All four current cases have zero speed excess and no lost formation membership.
Transient invalid lateral targets remain observable; the fallback prevents
persistent direct-slot seeking and all actors reach their exact final slots.

| Other case | Before | Current | Current last/ideal ticks | Current max still |
| --- | --- | --- | --- | --- |
| Two Generals, wooded diagonal bridge | 190/202 arrive; 21739 still | 202/202 | 7659/9994 | 47 |
| General + 100, wooded diagonal bridge | 101/101 (baseline) | 101/101 | 7644/8400 | 46 |
| General + 100, river detour | 101/101 (baseline) | 101/101 | 5578/4540 | 29 |
| General + 100, forest | 101/101 (baseline) | 101/101 | 3737/3103 | 32 |
| General army, enemy inside 13x13 sealed ring | 10/101 pass; 10655 still | 101/101 pass | not an arrival test | 25 |

The enemy ring exceeds all represented ranged weapons' reach. The enemy keeps
full HP. The individual attack-move enclosure test already passed before this
change; the new failure belonged to General fight decisions. The smaller
initial enclosure was an invalid army test because archers could shoot into it.
Bridge measurements are from `deck-centerline.log`; river/forest measurements
are from `squads-terrain.log` and need a final combined rerun.

## Causes and implementation

The merged baseline uses one ordinary formation mover for all three cases.
The General gives that mover a hundred members with role offsets; small groups
have less lateral extent, traffic and accumulated lag. There is no distinct
General pathfinding algorithm in that baseline.

- Carrier arrival and an open current tile could release a trailing member to
  its final slot across an obstacle. Keep the proven corridor until caught up,
  check forward clearance and fall back to the next centreline checkpoint.
- Short moving checkpoints prevented the existing friendly passing wait from
  applying to large formations. Use the retained final slot, preserve earned
  obstruction history and let stationary external traffic trigger recovery.
- Successful detached checkpoint completion after carrier arrival was mistaken
  for timeout. Rejoin successful movers; retain failure handling for real errors.
- The virtual carrier used raster diagonal checks where fractional physical
  deck checks were needed. Lateral offsets also pushed the final follower off
  narrow deck edges. Use continuous steps and the shared deck centreline.
- The fixed avoidance angles missed a legal fractional rear/lateral escape
  wedge among parked bodies. Two additional bounded candidate directions solve
  the reproduced wedge without lowering hard separation or exceeding speed.
- General fight orders targeted an inaccessible enemy centre without the
  ordinary reachable firing-position search. Share the existing sixteen-search
  per-tick cap, reject proved unreachable targets and resume the retained march.

Simulation squads have at most ten offices, filled in entity order: infantry
20, archers 15, cavalry (including scouts) 10, siege 5. The mixed hundred has
eight ordinary independently routed groups plus its commander. Paid pending
soldiers reserve typed capacity. A dead sergeant promotes the first survivor
without rearranging other squads. Heading stays stable across retries. Scouts
plan first; squads share one threat census. Independent squads catch up at their
own speed and use army pace beyond a twelve-tile lead. This is bounded synthetic
coverage, not 1000-player performance or dedicated hardware qualification.

## Remaining work and commands

The cold full-army three-bridge case now passes: 101/101, last tick 22097,
ideal 22200, maximum still 49, route work 780695, maximum per-tick route work
16384, 57 reversals, zero speed excess and no lost membership. Immediately
before the map correction it arrived 88/101 at 40000 ticks, with 24609 maximum
still and 34065883 route work. The first error was at tick 15152: the planner
certified (447,109) instead of (610,110). A flat-height shortcut omitted diagonal
bridge portals crossing a single chunk side because it assumed a cardinal
alternative existed. Side cells were water. Preserve explicit bridge edges in
that shortcut. The new minimal map test failed with nearest (31,22) instead of
(36,22); the corrected hierarchy suite passed 38/38. Extra regression coverage
now includes lazy, owned and packed navigation, both directions and destruction;
the extended suite passes 38/38.

`combined-green.log` ran 100 focused simulation tests: 95 passed, five stopped
at current-output equality assertions. All arrival/work/speed/membership/stall
acceptance checks passed. The one/four-worker replay agreed exactly on
`8e69490ccf200969444015ccd9a1803a1742ab391d386772780ea9cbe797c99a`.
The current-output pins were regenerated, preserving all acceptance bounds.
The final focused simulation rerun passes 100/100, including the new pins.
One of those tests covers the separate recording constructor, which is outside
this increment; all 99 core tests pass. Pre-commit rejected production panic
calls; fallible lookups replace them, and `panic-simulation.log` again passes
100/100 with the same exact replay and movement measurements. Formatting,
native/WASM lint and structure checks pass in `panic-static.log`.
These are deliberate tradeoffs,
not performance wins in every workload:

| Current-output case | Base last ticks | Current last ticks | Base/current route work | Base/current collision work | Base/current still/reversals |
| --- | --- | --- | --- | --- | --- |
| 40, forest | 3282 | 3282 | 1027/1027 | 2445117/2444034 | unchanged 4/0 |
| 80, opposing forest | 3278 | 3278 | 1933/1933 | 3097268/3096055 | unchanged 1/0 |
| 40, river | 5436 | 5527 | 55225/55473 | 5480816/5422483 | 7,2 / 5,2 |
| 100, thousand-tile march | 36705 | 35529 | 41486/378134 | 88820779/111559118 | 28,17 / 32,26 |

The long march now maintains independent routes and offsets for typed squads;
its increased total route work and reversals remain limits. Maximum per-tick
route work stays at the unchanged 16384 bound. Synthetic results do not qualify
1000 players or dedicated hardware.

`make hooks-install hooks-check` and intermediate `make fmt-check lint
structure-check` passed. A later fmt-check raced the formatter and failed;
sequential formatting, native/WASM lint and structure checks pass after moving
the local reset adapter out of the map module to keep it below 500 lines.
Final log: `reset-static.log`. Compilation/checks
use Dockerized Make targets, with logs in ignored `reports/squads/`.

The protocol's old bound rejected a valid 200-infantry army. The updated
maximum-size wire regression failed red, then passed with production validation
updated to 200; it still rejects 201. Logs: `protocol-capacity-red.log` and
`protocol-capacity-green.log`. The basic HUD removes its obsolete
100-soldier capacity label and describes ten squads. WASM-cap validation passes at 375226 compressed bytes (+23.9% versus
302817, below the unchanged +25% block). Browser validation is pending. No per-sergeant configuration has been implemented.

The seven locked geographic inputs were downloaded and SHA-verified with
`make geodata-bootstrap`. `make assets-import-de` imported native original art
outside Git. `make map-generate-detailed` generated real-source Paris package
`e0a0c70a58577d2731b926c29ec1eb988411d8eb2132c21fe814e2b7cd142b77`.
The map request is 30 km, compression 30:1, 600 CE, seed 1. Source/art data and
pixels remain ignored. A local recording constructor adds a General, 100 mixed
soldiers and a completed bridge to the existing map. It samples clear spawn
positions without replacing terrain. Its first simulation setup test passed;
the woodland-aware spawn selection passes in the final 100-test simulation run. The source server starts at 127.0.0.1:8205 with the original imported pack;
its immutable navigation bake maps 256 chunks (337818 retained bytes, 4168 heap
bytes) off tick. This is a small-map observation, not a heap/RSS qualification.
The recording setup finds the original river at (231,252) to (248,252), keeps its
water pages and issues no march until browser input. The first browser attempt failed because the DE overlay was selected without
its required trial base pack. It never connected and is not crossing evidence.
The local base-pack preparation is in progress. No crossing video has yet been
inspected.

Sergeant hero appearance and configuration UI follow a passing simulation
slice. They are not implemented. The required close-camera real-source river
video and first/middle/last-frame inspection remain outstanding. Preflight,
perf-ci, full E2E, publication, owner servers and other checkouts are excluded
by the explicit brief. No source-map performance claim is made.
