# Roadmap: Generals, Vassals, Stewards and the management hierarchy

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Steward and Vassal action areas still drawn and felt as perfect circles; no visible change in Steward or Vassal behaviour after #962 and #975 | 11-22 x1 | PARTIAL: in this PR | Owner playtest 11-22 (Anatolia 4 converted, dev build play/21, 73.6k units). The client outline (crates/client/src/village/steward/territory.rs) still draws the old nearest-centre circle cells; the server zone is walking-distance ownership clipped to the 100-tile service radius. 11-23 code reading (dev 11-22): only resource-patch ownership uses walking distance (crates/simulation/src/land/walking/field.rs, 100 tiles); zone membership (`Home::contains`, steward/mod.rs) is a Euclidean circle and discovery is clipped to it, so on real maps full of detours the walking-distance zone is not implemented | Server: membership, resource discovery and rally checks come from the walking field (100 walking tiles), no Euclidean clip; while a player's offices change, zones fall back to nearest-centre circles until re-walked (within a second), so a neighbour's stale zone never takes villagers. Next: the client outline (territory.rs) still draws circle cells; send the zone shape from the server |
| Vassal bigger redesign: staff villagers (0/20 after hours), priority over Stewards, scale staff to workload, sliders (focus, build speed), show current/planned/finished work | 11-09 x1 (sliders asked 10-30 too), 11-20 | SUPERSEDED 11-20 | #669 per-Vassal controls, #762 census stall fixed, #694 crews. Staffing and planned/finished display not found in log; staffing branch `night/vassal-redesign` redirected 11-20 | See the 11-20 row above |
| Vassal roads: trunk routes linking production, centre, gates; bridges first | 11-05, 11-06, 11-09 x3 | PARTIAL | #693, #721 (roads, a bridge), #750 budget | Trunk-road planner and bridge priority |
| Vassal calls Generals from too far (5 across a river) | 11-09 x1 | PLANNED | #688 uses 100-tile radius; owner says too large | Shrink radius and require reachable path |
| What Vassals do with their 20 defence units | 11-09 x1 | PLANNED | Unclear to owner; no PR | Document and show in UI |
| Steward and Vassal zones in walking distance (1 s recompute; whole resource patch belongs to the zone) | 11-05, 11-09 x2 | PARTIAL | #704 stays inside walkable territory | Walking-distance zones, patch ownership |
| Management hierarchy: office engine, provinces from geodata, Army/Economy/Navy/Church/Intel/Works/Expansion tracks, intents down and reports up | 10-31, 11-03, 11-08 x3, owner: "later" | PLANNED: spec agreed 11-25 | Design agreed with the owner 11-25: [spec](../hierarchy/spec.md) (provinces from geography, the 10-31 ladders, offices play unwatched territory, all seven tracks, Rust rulers) | Build order in the spec, section 10: provinces first |
| Improvement tournaments for every office (Generals, Vassals, Stewards and the other ranks) | 11-25 x1 | PLANNED | Owner 11-25: the same loop as AI tournaments, optimising an office's ability to reach an objective (conquer a province, defend a border, collect resources) and its cost in instructions. One objective scenario per office type, scored on objective reached, time and instructions; racing and a Pareto front | After the hierarchy (spec section 10) |
| Sergeants: one hero per type (infantry 20, archers 15, cavalry 10, siege 5), General leads 10 squadrons | 11-03 x1 | PARTIAL | #668, #689 (typed squads and rosters); hero assets for sergeants not found | Check after the hierarchy decision |
| Province map as far view; LLM counsel as top manager; light supply far from own provinces | 10-31 | PLANNED | Open question to owner, unanswered | Later |
| Vassals and Generals desync/override owner orders | 11-03, 11-09 | PARTIAL | #711, #800 | Verify on real terrain |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| Vassals do not own villagers: they place foundations and villagers come to build them, like far-view placement; balance build speed (far-view buildings get too few builders) against the economy (never all villagers building); Vassals and Stewards cooperate badly and leave villagers idle | 11-20 x1 | DONE | #962 (Vassals place foundations, shared builders raise them, no Vassal-owned villagers); owner confirmed done 11-23 |
| The General: Castle leader, 50-tile sphere, Captain/Trainer modes, mix sliders | 10-29, 10-30 | DONE | #633, #662 |
| Generals adopt free soldiers on creation; independently ordered soldiers leave the army | 11-09 | DONE | #800, #711 |
| Steward takes over TC economy; Vassal defends and fortifies land | 11-03 | DONE | #685, #688 |
| Vassal stability: churn, farm tiling, adoption, split overlapping territory | 10-30, 11-03 | DONE | #641, #648, #680 |
| Vassal "too many units nearby" census stall; Stewards delete old lumber camps | 11-10, 11-09 | DONE | #762 |
| Steward stuck villagers recovered, farms when food dry | 11-05 | DONE | #704 |
| Walled Steward-sized domain with roads and a bridge | 11-05 | DONE | #721 |
