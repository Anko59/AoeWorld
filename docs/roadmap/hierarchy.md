# Roadmap: Generals, Vassals, Stewards and the management hierarchy

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Vassal controls: three independent sliders for fortification strength, road density and building speed | 10-30, 11-09, 11-28 x3 | PLANNED | #1025 widened the controls and renamed Focus to Development priority. Owner 11-28: "one slider that acts like a toggle and is unclear" | Replace the current control with three sliders with visible effects and tooltips. Verify that intermediate values affect fortifications, roads and building speed independently. |
| Vassal bigger redesign: staff villagers (0/20 after hours), priority over Stewards, scale staff to workload, show current/planned/finished work | 11-09 x1, 11-20 | SUPERSEDED 11-20 | #669 per-Vassal controls, #762 census stall fixed, #694 crews. Staffing and planned/finished display not found in log; staffing branch `night/vassal-redesign` redirected 11-20 | See the 11-20 shared builders row below. |
| Steward and Vassal zones in walking distance (1 s recompute; whole resource patch belongs to the zone) | 11-05, 11-09 x2 | PARTIAL | #975 and #1038 landed walking-distance membership and patch ownership. #1009 refreshes terrain reach every eight game seconds, with office changes and new bridges picked up within one second. | The requested one-second terrain refresh differs from the merged cadence; client outlines landed in #1136. Recheck in play. |
| Management hierarchy: office engine, provinces from geodata, Army/Economy/Navy/Church/Intel/Works/Expansion tracks, intents down and reports up | 10-31, 11-03, 11-08 x3, owner: "later" | PLANNED: spec agreed 11-25 | Design agreed with the owner 11-25: [spec](../hierarchy/spec.md) (provinces from geography, the 10-31 ladders, offices play unwatched territory, all seven tracks, Rust rulers). Step 4 brought forward on 11-25 as a port of today's AI (no offices yet): the Python AI's strategy runs in Rust in the server by default, Python stays for tournaments ([AI](ai.md)) | Build order in the spec, section 10: provinces first; step 4's rulers then issue intents to offices instead of orders |
| Improvement tournaments for every office (Generals, Vassals, Stewards and the other ranks) | 11-25 x1 | PLANNED | Owner 11-25: the same loop as AI tournaments, optimising an office's ability to reach an objective (conquer a province, defend a border, collect resources) and its cost in instructions. One objective scenario per office type, scored on objective reached, time and instructions; racing and a Pareto front | After the hierarchy (spec section 10) |
| Sergeants: one hero per type (infantry 20, archers 15, cavalry 10, siege 5), General leads 10 squadrons | 11-03 x1 | PARTIAL | #668, #689 (typed squads and rosters); hero assets for sergeants not found | Check after the hierarchy decision |
| Province map as far view; LLM counsel as top manager; light supply far from own provinces | 10-31 | PLANNED | Open question to owner, unanswered | Later |
| Vassals and Generals desync/override owner orders | 11-03, 11-09 | PARTIAL | #711 and #800 improved command ownership. #972 keeps General armies on deliberate owner orders. | Verify remaining distance-related desync and Vassal overrides on real terrain. |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| Steward and Vassal action areas still drawn and felt as perfect circles; no visible change in Steward or Vassal behaviour after #962 and #975 | 11-22 x1 | DONE | #1038 moved server membership, discovery and rally checks to the walking field. #1136 merged 11-27 at 07:21 CET. The detailed and strategic views draw the server walking shape. Recheck in play. |
| Vassal roads: trunk routes linking production, centre, gates; bridges first | 11-05, 11-06, 11-09 x3 | DONE | #975 plans shared trunk roads and queues bridge foundations first. Recheck in play. |
| Vassal calls Generals from too far (5 across a river) | 11-09 x1 | DONE | #975 calls Generals within 32 tiles of the Castle only when they can reach the threat and have no active owner order. Recheck in play. |
| What Vassals do with their 20 defence units | 11-09 x1 | DONE | #975 shows each defence squad’s current activities in the Castle panel. Recheck in play. |
| Vassals do not own villagers: they place foundations and villagers come to build them, like far-view placement; balance build speed (far-view buildings get too few builders) against the economy (never all villagers building); Vassals and Stewards cooperate badly and leave villagers idle | 11-20 x1 | DONE | #962 (Vassals place foundations, shared builders raise them, no Vassal-owned villagers); owner confirmed done 11-23 |
| The General: Castle leader, 50-tile sphere, Captain/Trainer modes, mix sliders | 10-29, 10-30 | DONE | #633, #662 |
| Generals adopt free soldiers on creation; independently ordered soldiers leave the army | 11-09 | DONE | #800, #711 |
| Steward takes over TC economy; Vassal defends and fortifies land | 11-03 | DONE | #685, #688 |
| Vassal stability: churn, farm tiling, adoption, split overlapping territory | 10-30, 11-03 | DONE | #641, #648, #680 |
| Vassal "too many units nearby" census stall; Stewards delete old lumber camps | 11-10, 11-09 | DONE | #762 |
| Steward stuck villagers recovered, farms when food dry | 11-05 | DONE | #704 |
| Walled Steward-sized domain with roads and a bridge | 11-05 | DONE | #721 |
