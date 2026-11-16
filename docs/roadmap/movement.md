# Roadmap: movement and pathfinding

Owner judgement is by play. Metric rule: instructions per tick on `make save-replay` plus `make quality-metrics`
(see [rules](rules.md)). Owner 11-13: replay-hash equality is no longer required.

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Long routes die, units go back and forth waiting for a route | 11-05, 11-06, 11-09, 11-15 x4 | PARTIAL | #716, #705, #849 (fights get a route slot first). Owner 11-15 min 115: "Long routes are 100% dead". Counters at tick 134,685: slot_refusals 7.86M, 897 waiting without planner, oldest wait 3,231 ticks. Branch `fix/route-starvation` (3 commits) unmerged | 11-16 game: 9,000-10,000 waiting, about 9,500 without a planner, oldest always at the 3,600-tick expiry. Next: scale queue Q2 on the Q1 scoreboard ([scale](scale.md)) |
| Formation and group coherence on real terrain (rivers, forests, bridges, chokepoints) | 10-28, 10-29, 10-30, 11-03, 11-05, 11-06, 11-10, 11-15 x8 | PARTIAL | #627, #628, #643, #668, #682, #710, #727, #729, #731, #733. Metrics 11-14: formation out of shape 61.7-100%, 4,876 obstructed ticks per unit on `long`, General worst slot deviation 23.95 tiles. `perf/movement-unify` has 5 unmerged commits | Re-measure after #846; finish movement unify under the +2% war-replay budget AND the gated late_game counter |
| Steward-ordered transfers arrive 80-82% (others 100%) | 11-14 x1 | PARTIAL | Real finding of `quality-metrics`; #846 delivers individually ordered transfers through the shared mover, not re-measured | Run `make quality-metrics`, put before/after in the PR |
| Generals and armies cannot path through the owner's own gates; Generals look-capped (ignore some production buildings in range) | 11-15 x1 | PLANNED | Owner report 11-15; earlier gate fixes #715, #777 only cover owners' units | Check in code first; add a gate-crossing scenario to quality-metrics |
| Phase 4 cutover (corridors, shared direction fields, no route queue) | 11-07 x1 plan, 11-09, 11-12 | PARTIAL | Groundwork library on `rework/phase4-graph` not on dev; `rework/phase4-cutover` 75 commits, measured slower than dev (365M vs 322M instr per tick), parked 11-13. Owner 11-12: judge only by ticks/s on a war state; ship or cut in hours | Decided by scale queue Q6 (squad as one body, phase 4 post-mortem) |
| Phase 5 formation frames | 11-07 | PARTIAL | `rework/phase5-frames` 17 commits ahead, unmeasured; WIP saved as backup commit 3fd9a5e06; port is thousands of lines | Measure on the war replay or cut; depends on phase 4 decision |
| Order-volume cuts (scout, Vassal, Steward, General re-issues) | 11-10 x1 | PARTIAL | #834 attributes order volume by issuer; #788 keeps routes on repeated orders. Branch `perf/order-volume` no longer exists, cut gain not recorded | Read the #834 report on the owner save, fix top issuers at source |
| Formations file into a line instead of walking wide around obstacles | 11-10 x1 | PLANNED | Owner lever 3; needs phase 4/5 | Tied to phase 4/5 decision |
| Bridge code audit and bridges as ordinary chokepoints; merge four enemy scans; three direct writers of order fields | 11-10, 11-13 audit | PLANNED | No PR | After order-volume work |
| Phase 3 leftovers: 3-bucket union-find churn under 2 ms, 52 ms flush, live march probe | 11-09 | PLANNED | #743 merged; leftovers listed in the 11-09 plan, none verified | Re-measure, then decide |
| Owner's five weaknesses (mixed groups straight-line, shape, route drops, partial obstacles, left-right oscillation) | 11-06 x1 | UNVERIFIED | #727, #729, #731 target them; owner 11-10: "much more successful, not enough" | Re-check each on a real scenario after route starvation fix |
| Units stuck in tree patches, lost at rivers, villagers stuck "Moving", idle after forest empties | 11-09 | UNVERIFIED | Flagged "tick symptoms, recheck"; not re-verified | Recheck after the route starvation fix |
| Generals and armies desync, control lost at distance; share sightings; shift-queue for Generals | 11-09 | PARTIAL | #711 (order takes soldier out of army), #800 | Verify desync on a 50-tile General scenario |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| Radius movement (no tile exclusivity) and group block marching | 10-28 | DONE | #606, #628 |
| Armies recover at rivers, chokepoints, one lane; typed squads cross bridges | 10-30, 11-05 | DONE | #710, #668, #689 |
| Walk animation follows movement (moonwalk, x3) | 10-30, 11-05 | DONE | #707 |
| Regions, reachability labels | 11-07 | DONE | #743 |
| Shared ground answers, clearance reuse, navigation points | 11-13 | DONE | #817, #793, #812 |
| Parallel apply (radix, Jacobi) | 11-09 | DROPPED | Slower twice (11-10: 5x/1 worker 256.9 to 323.7 ms); do less work instead |
| Phase 2 avoidance as a perf change | 11-09 | DROPPED | +0.43% instructions (11-14); the pair-passing and fishing-ship deadlock fix is parked as `parked/phase2-avoid-passing-2026-11-14.patch` |
| Parallel route serving | 11-14 | DROPPED | `perf/parallel-plan-serve`: late_game +34.9% against a 3% gate, refusals 42k to 118k |
| Route search over cached walk chunks | 11-09 | DROPPED | Held 5% slower; phase 4 would replace it |
| Commute route cache and short approach | 11-10 | DROPPED | No gain, see issue 761 |
| Full from-scratch group redesign | 11-06 | SUPERSEDED | Owner 11-06 "no redesign", then 11-07 asked for the once-and-for-all plan (docs/movement); the plan's deliverables are the rows above |
