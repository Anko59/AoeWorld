# Squads brief: reproduction and delivery evidence

Work started 2026-11-03 02:28 UTC, branch `movement/squads`, with an
unfinished merge at `d2822c783406c739594aa2a3374147944a17bc40`.
The conflict-resolved merge is `2522ae0b1f9fb37582da00e8419731c881e82569`.
It preserves General-only follower yielding and single-file bridge recovery.
The combined baseline's independent one/four-worker replay agrees exactly on
`575a774847525864239b77dcd34908aae5fe73b093b3c239e39061b06325ac11`.

The verified squad/bridge increment is
`c3fb64d27a9151c6992c3fcbcd5666fe965df568`. Its pre-commit hook passes on
isolated index tree `84decd6cf00721d4ca3361ef0a63448252976cac`.
This follow-up adds the local source recording setup and basic capacity wording.

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

## Final checks and follow-up evidence

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
302817, below the unchanged +25% block). `make browser-format browser-check` passes; the real-source browser proof passes. No per-sergeant configuration has been implemented.

## Real-source recording

The seven locked base geographic inputs were downloaded and SHA-verified with
`make geodata-bootstrap`. `make map-generate-detailed` generated the Paris map
`e0a0c70a58577d2731b926c29ec1eb988411d8eb2132c21fe814e2b7cd142b77`:
30 km, compression 30:1, 500 tiles per side, 600 CE, seed 1, 19 source locks.
The setup samples this existing map, adds a completed bridge from (231,252) to
(248,252), and spawns 100 mixed soldiers and their General on clear ground.
It preserves source water pages and leaves the march to ordinary browser input.
The recording uses WebGL2 software rendering, zoom 0.4, and a four-times clock
(world tick rate 5 under the server's 20 Hz ticker). No owner server was touched.

The first browser attempt selected the standalone DE overlay and failed the
required base-art check. A second attempt exposed an off-screen camera choice.
Neither is crossing evidence. The successful setup uses the trial base pack
`54916fcb9c1e43c7a63aa486e3c47824b24ba94c887168ed210517273626bff6`
and DE overlay
`5a0a9945b34c693e4f979210f5b5803a5dbb5a29dfc6075db9be1b88ad676f49`.
The extracted trial installer matches the documented 49083656-byte SHA-256
`0f5df430b54a377a6cfd1e253169de2cf8a60a1751d6e8b72718d297bd45174c`.
Source assets, generated packs and video pixels stay ignored and outside images.

Frame inspection also caught a setup population error: removing a previous
village's units without death cleanup retained their population. The strengthened
fixture fails 206 versus 101, then passes with normal death cleanup. The refreshed
recording shows population 101. Native setup test, six focused server tests,
browser checks, static checks and WASM-cap pass. Logs are in `reports/squads/`:
`recording-village-population-red.log`, `recording-population-green.log`,
`server-final.log`, `recording-browser-check.log`, `recording-static.log`,
`recording-final-wasm-cap.log`, and `source-final-video.log`.

The 59-second clip is `reports/squads/general-river-crossing.webm`, SHA-256
`40866da79ecca42468cadf41a4bc6ab8d57da6653cdff4def0dd105fd0dc7b57`.
It is a continuous excerpt starting 43.4 seconds into the 102.4-second raw
Playwright capture, removing startup. `reports/squads/river-video.json` retains
settings and hashes. The extracted first (0 s), middle (29.48 s) and last
(58.96 s) frames in `reports/squads/video-frames/` were inspected: the army files
onto the deck, units reach the far bank, and the squads spread beyond it.
The camera remains close enough to distinguish individual units. This is visual
crossing evidence; no real-source before measurement or exact arrival-time claim
is made. The clip was built from the squad commit plus the recording setup,
before the separate first-sight recovery follow-up.

Sergeant hero appearance and per-sergeant/same-type-group unit-mix controls
remain unimplemented. The existing General's role sliders remain available.
Preflight, perf-ci, full E2E and publication are excluded by the explicit brief.
No 1000-player, dedicated-hardware or finished RTS performance claim is made.

## Unfinished recovery follow-up at the budget stop

Two additional boundary tests found failures in the committed squad code:

| Case | Passed beyond enclosure | Longest still ticks |
| --- | --- | --- |
| Unreachable enemy visible at the first march order | 101/101 | 318 |
| Enemy enclosed after a fighting target was acquired | 0/101 | 5821 |

The terrain-changing fixture explicitly invalidates collision/navigation caches.
`first-sight-invalidated-red.log` retains this baseline. An attempted heading
fallback and failed-route retry only reached 90/101 in the second case and left
312 still ticks in the first. It is not a verified fix. The two new draft tests
and that attempt are saved in `reports/squads/unverified-first-sight-followup.patch`;
they are excluded from the delivered tree at the owner's explicit budget stop.
No existing accepted test or acceptance bound was removed or weakened.
The source recording contains no enemy and predates this independent attempt.

Remaining work includes these two recovery boundaries, sergeant hero art and
per-General/per-sergeant/same-sergeant-group unit-mix configuration. The task
is incomplete; the verified commits and video do not establish those features.
