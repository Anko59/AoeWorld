# Roadmap

The single status-tracked list of what the owner asked for and what we agreed. Reconciled 2026-11-16 from issue 748
(body and comments to 11-14), issues 745 and 265, the owner-direction notes (playtests 10-26 to 11-15, roadmaps,
night and day briefs) and checked against `origin/dev` at `ec5bf2e15` (#857, 2026-11-14) and the pull request list.
`make roadmap` prints the open items and counts; `make roadmap-check` fails when a DONE row names no merged PR.

## How to read and keep it

- One row per distinct ask. Repeated asks are one row with "xN". Dates are 2026 (MM-DD).
- Status is the first word of the status cell: DONE (merged PR number required), PARTIAL (say what landed and what not),
  IN FLIGHT (named branch), PLANNED (agreed, not started), DROPPED (decision and date quoted), SUPERSEDED, UNVERIFIED
  (claimed done or open, no merge evidence either way).
- Open items are ordered by owner emphasis, most urgent first. Done and dropped items sit in the second table of a file.
- Update this file in the same change as every merge, drop and new owner ask. Never write DONE from a note alone.
- Areas: [gameplay](roadmap/gameplay.md), [movement](roadmap/movement.md), [AI](roadmap/ai.md),
  [scale](roadmap/scale.md), [hierarchy](roadmap/hierarchy.md), [UI](roadmap/ui.md), [map](roadmap/map.md),
  [harness](roadmap/harness.md), [caps to remove](roadmap/caps.md), [standing rules](roadmap/rules.md).

## Decisions blocking the 2M-unit target (owner to decide)

Owner 11-24: "priority goes to any decisions we still have to take that block the 2m units objective". Raised 11-24;
1, 3 and 4 decided 11-25, 2 and 5 still open; details and evidence in [scale](roadmap/scale.md).

1. **Unseen-unit fidelity: decided 11-25.** Unwatched territory is played by its offices as groups ([hierarchy
   spec](hierarchy/spec.md), section 6, office mode). Acceptance is plausibility on the 5x and 10x benches (kills within
   about 20%, stock within about 15% over five minutes), not the 0.10 parity bound. This replaces #978's off-screen
   design (its section 4.3, decision 5 and the parity bound); #978's rules, State comment and steps 6, 8 and 11 stay.
2. **Movement architecture past 500k.** The rewrite core was killed 11-14 (28,500 instructions per mover against a
   2,000 gate); "decided by Q6" was never decided. Without a direction, rungs 2-3 have no plan.
3. **Hierarchy engine: decided 11-25.** Specification agreed with the owner ([spec](hierarchy/spec.md),
   [hierarchy](roadmap/hierarchy.md)); build order in its section 10, provinces first.
4. **Memory per unit: target set 11-25.** 4 KB per unit (2M units in 14 GB with the OS and AIs). An earlier 5x bench measured
   7–8 KB per unit; #1075 saved about 240 MB of heap at 5x by boxing the planner. Reprofile total memory on the 5x save.
5. **Players and AI cost.** Python AIs cost about 47 ms CPU per decision (#942); 2M units with 99 AIs or with hundreds of
   players decides whether AI decisions move into Rust.

## Ten highest-priority items from the 11-26 brief

Order follows the owner's 11-26 emphasis. Reconciled 2026-11-28 against the first-parent merge log of `origin/dev` through `43546734ccf6`.
The owner is away 2026-11-27 to 2026-11-30.

Last playtest, 11-25 (play25, dev 2d7ae70aa, 99 Python AIs): the converted Anatolia 5 save was "absolutely
unplayable" and was dropped. The new France game was "a pleasure, still nearly no lag" at 63.5k units and
6.0 ticks/s until the client crashed and leaked memory. On 11-26 the owner said fights and AIs are
"in a good state and not priority" and singled out client stutter.

1. Client stability: terrain crash **DONE** via #1111. The Firefox WebGPU leak remains open at about 90 MB/min;
   diagnosis needs an owner Firefox `about:memory` capture during growth ([UI](roadmap/ui.md)).
2. Route caps **DONE** via #1096. R1 never-moved units fell from 10,429 to 3,709 at +71% instructions/tick.
   Recheck in play. Removing the remaining route queue is item 3 ([scale](roadmap/scale.md), [caps](roadmap/caps.md)).
3. Movement cost **PARTIAL**. #1122 moved lookahead to workers and kept planners across coarse-only chunk overlaps
   (main-thread instructions/tick: R1 −7.9%, 5x −16.6%, arrivals effectively unchanged). Shared paths and removal of the remaining queue are still ahead
   ([movement](roadmap/movement.md), #978).
4. Late-game server lag **PARTIAL**. France reached 6.0 of the target 10 ticks/s at 63.5k units on 11-25, up from
   4.0 at 61.5k on 11-19. Measure a live owner game with default Rust AI and route caps removed ([scale](roadmap/scale.md)).
5. Wasted unit work **PARTIAL**. #1114 merged reachability checks for gathering and construction and suspends
   occupied foundations. Attack-move still needs to engage enemy buildings in its path ([gameplay](roadmap/gameplay.md)).
6. Client stutter **PARTIAL**. #1112 improved France 11 crowded-hold p99 from 10,145 to 127 ms and pan p99 from
   1,487 to 793 ms. #1129 merged terrain preparation ahead of fast pans, with pan median 125 to 37 ms
   in its benchmark. Recheck long frames in play ([scale](roadmap/scale.md)).
7. Formation coherence and Steward arrival **PARTIAL**. Wide formations still file into long lines on real terrain.
   The Steward transfer baseline was 80–82%; rerun quality metrics with the merged route changes ([movement](roadmap/movement.md)).
8. Walking-distance zones and office cooperation **PARTIAL**. Server walking membership landed in #1038 and shared
   Vassal builders in #962. #1136 merged client outlines in both views. Recheck in play
   ([hierarchy](roadmap/hierarchy.md)).
9. Other caps and budgets **PARTIAL**. #1029 removed global attack-position search caps and #1038 removed global
   office visit and per-visit limits. #1140 removed fixed limits for General admissions, Vassal plans,
   wall orders and builders looking for a next foundation. Remaining caps follow the owner's "solve, not limit and hide" rule ([caps](roadmap/caps.md)).
10. Fights and AIs **PARTIAL**, deprioritised by the owner on 11-26. #1026 removed engagement caps and #1034 improved
    march response and defence. Engaged armies still stack too tightly, and late-game attacks away from home need a
    play recheck with the Rust AI ([gameplay](roadmap/gameplay.md), [AI](roadmap/ai.md)).

Next after these: recheck merged rock footprints (#926), DE road edges, far-view training flow and enemy foundation bars
in play. Medieval DE styling remains for the training and activity panels. #974 merged bare furrows for unseeded farms;
recheck in play. The hierarchy build order stays provinces first.

## Merges since 2026-11-26 17:00

- Client: crash fix #1111, stutter round 1 #1112, map recovery and browser test fixes #1131, #1127 and #1134,
  plus walking-zone outlines #1136.
- Simulation and AI: worker route lookahead #1122, seed 4 AI recovery #1125 and Rust seat timing #1097.
- Smirkey: minimap raster #1081 (19× faster on a 2,000-tile map, identical bytes), bulk housing recount #1095
  and minimap orders #1100.
- CI: selective PR checks #1121, read-only Docker cache on PRs #1126, per-leg Rust caches #1132 and job setup #1133.
  Also map cache recovery #1124, nightly triage #1116, native timing assertions #1119, browser repeat/retry #1113/#1130,
  scheduled dependency audit #1123, Anatolia package command #1118 and dependency updates #1110.
- Harness and docs: real-map game score #1128, CI report #1115 and 11-26 owner feedback #1104.

## Roadmap changes reconciled on 2026-11-28

- #1114 closed the unreachable-work and occupied-foundation items. Attack-move through enemy buildings remains open.
- #1151 increased gold per pile and doubled Outpost sight. #1160 landed resource regrowth and recovery.
  #1161 landed relics. #1165 measured the France pacing baseline for the open balance work.
- #1135 and #1142 added local ship comparisons and staged ford scenarios to the game score.
- #1025 landed the far-view training workflow, Shift button placement, wide management controls and DE roads.
  #974, #957, #1020, #969 and #961 also have merged UI rows. Styling and reports awaiting play verification
  remain in the open table.

## Source conflicts resolved in this roadmap

- Issue 748 comments of 11-13 and 11-14 list "fights ~60% engage, animations, map-creation ETA, AI tournament, Vassal
  stability" as not started. The owner confirmed them done on 11-11 and the merge log has the PRs (#719, #707, #717,
  #723, #680). Here: DONE where merged; fights stay PARTIAL because the owner saw non-engagement again on 11-15.
- Issue 748 body (11-09) still lists #743, #744, #747, parallel apply, S1 and walking gatherers as pending. All are
  merged, dropped (parallel apply, S1) or landed (#773). The body is a snapshot; this file replaces it.
- Issue 745 comments: item 5 shipped (#767), items 9, 12, 18 and the farm half of 6 fixed (#751, #764), item 1 does not
  reproduce. The 11-13 roadmap comment still called 1, 2 and 5 open.
- The 11-14 comment "a player's attack-move does not fight in melee" was a harness bug and was retracted; only the
  #828 regression was real (fixed in #840).
- Owner 11-12 "judge only by ticks/s on a war state" was replaced the same day by instructions per tick; the second rule stands.
- Owner 11-06 "no redesign of pathfinding" was reversed on 11-07 ("once and for all"); the later plan stands but its core
  (phase 4 cutover, frames) never shipped and is parked.
- Harness movement metrics (single units 100% arrival) disagree with owner play on 11-15 ("long routes dead"). Treat the
  owner report as true and the metric scenarios as too easy until reproduced from the game's own save.
- Owner 11-15 reports gates, new content and HUD broken although #715, #777, #855 and #857 are merged; those PRs prove
  merge, not play. Rows stay open until a played build confirms.
