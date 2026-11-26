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
4. **Memory per unit: target set 11-25.** 4 KB per unit (2M units in 14 GB with the OS and AIs). The 5x bench measures
   7-8 KB per unit today; a heap profile of the 5x save comes first.
5. **Players and AI cost.** Python AIs cost about 47 ms CPU per decision (#942); 2M units with 99 AIs or with hundreds of
   players decides whether AI decisions move into Rust.

## Ten highest-priority open items

Open items only, owner emphasis first (reconciled 2026-11-26 against `origin/dev` at `84acf02f2`, the open pull
requests, the owner playtest of 11-25 and the owner's feedback of 11-26).

Last playtest, 11-25 (play25, dev 2d7ae70aa, 99 Python AIs): the converted Anatolia 5 save was "absolutely
unplayable" (client under 1 FPS, route caps, fords changed by #1036) and was dropped; a new France game was "a
pleasure, still nearly no lag" at 63.5k units and 6.0 ticks/s, until the client crashed and leaked memory. Owner
11-26: fights and AIs are "in a good state and not priority" (AI armies that walk without engaging went from about 90%
to 30% of the time); client stutter, not average frame rate, is what frustrates.

1. Client stability: a terrain panic crashed the client on repeat (fix on `client/panic-message`), and the WebGPU tab
   grows about 90 MB a minute until Firefox runs out of memory ([UI](roadmap/ui.md)).
2. Route caps and the route queue (owner 11-25: "The route caps are still not removed so I can't do shit"; asked since
   11-05): route caps off is shipping on #978 (#1096) at +71% instructions per tick, never-moved units 10,429 to 3,709
   on R1 ([scale](roadmap/scale.md), [caps](roadmap/caps.md)).
3. Movement simplified and its cost brought back down (owner 11-26): keep planners across building placement, shared
   paths per player and destination chunk, then delete the route queue ([movement](roadmap/movement.md), #978).
4. Server lag in big late games: 11-25 France 6.0 ticks/s at 63.5k units (60% of real time at 10 Hz), up from 11-19's
   4.0 at 61.5k; target real time at 10 Hz. The next owner game is the first with the Rust AI and without route caps,
   so measure it again ([scale](roadmap/scale.md)).
5. Wasted unit work the owner saw: attack-move paths around enemy buildings instead of destroying them; Steward
   villagers gather resources they cannot reach; builders sent to unreachable foundations or to one an enemy stands
   on, dozens at a time ([gameplay](roadmap/gameplay.md)).
6. Client stutter in crowded views and fast pans: measure long frames, not the average (28 FPS average at 37.8k units
   on France) ([scale](roadmap/scale.md)).
7. Formation coherence on real terrain (11-20, 11-22: huge lines) and Steward arrival 80-82% ([movement](roadmap/movement.md)).
8. Steward and Vassal zones really in walking distance (membership and discovery are still a circle; 11-23), Vassal and
   Steward cooperation ([hierarchy](roadmap/hierarchy.md)).
9. Remove the other caps and budgets (owner 11-23 "solve, not limit and hide") ([caps](roadmap/caps.md)).
10. Fights and AIs, not a priority (owner 11-26): own engaged armies stack too tightly; AIs rarely leave their base
    after the early game ([gameplay](roadmap/gameplay.md), [AI](roadmap/ai.md)).

Next after these: rock footprints (#926, held only while the owner needed the Anatolia 4 save, which the owner no longer
plays), the 11-22 UI items still open (far-view training panel and activity panel in DE style, road texture and enemy
foundation bars to recheck in play), unseeded farm furrows, the hierarchy build order (provinces first). Done since
11-21: shallows and saved resource memory (#1036), uncapped far view and order limits (#1020, #1028), Vassals without
villagers (#962), uncapped attack-move looks (#1026), the Rust AI by default (#1080, #1092).

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
