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
  [harness](roadmap/harness.md), [standing rules](roadmap/rules.md).

## Ten highest-priority open items

1. Server lag in big late games: 6.9 ticks/s at 16.7k units (11-15), 3.14 at game minute 153; target 20 ([scale](roadmap/scale.md)).
2. Long routes "100% dead", units shuffling while waiting for a route; fights-first route slots merged in #849 but long routes still die in play ([movement](roadmap/movement.md)).
3. Armies near enemies not engaging ([gameplay](roadmap/gameplay.md)).
4. Generals and armies cannot path through the owner's own gates; Generals look-capped ([movement](roadmap/movement.md)).
5. New content (navy, land units, gunpowder) merged and playable end to end; HUD name/icon bugs ([gameplay](roadmap/gameplay.md)).
6. Formation coherence on real terrain and Steward arrival 80-82% ([movement](roadmap/movement.md)).
7. Much stronger scaling AIs that upgrade and use new content ([AI](roadmap/ai.md)).
8. Client frame rate in big games (53 ms median frame at 16.7k units) ([scale](roadmap/scale.md)).
9. Vassal redesign: staffing, sliders, trunk roads and bridges first, General-call radius, walking zones ([hierarchy](roadmap/hierarchy.md)).
10. Decide the future of the movement rewrite (phase 4 and 5 parked, S1 killed) and rung 2 ([scale](roadmap/scale.md)).

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
