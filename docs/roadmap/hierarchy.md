# Roadmap: Generals, Vassals, Stewards and the management hierarchy

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Vassals do not own villagers: they place foundations and villagers come to build them, like far-view placement; balance build speed (far-view buildings get too few builders) against the economy (never all villagers building); Vassals and Stewards cooperate badly and leave villagers idle | 11-20 x1 | IN FLIGHT `night/vassal-redesign` | Owner playtest 11-20 (Anatolia 4, play/20 build, 73.6k units). Owner changed the 11-09 staffing design. | Remove Vassal staffing; shared bounded builder pool for Vassal and far-view foundations; measure foundations per minute and idle share |
| Vassal bigger redesign: staff villagers (0/20 after hours), priority over Stewards, scale staff to workload, sliders (focus, build speed), show current/planned/finished work | 11-09 x1 (sliders asked 10-30 too), 11-20 | SUPERSEDED 11-20 | #669 per-Vassal controls, #762 census stall fixed, #694 crews. Staffing and planned/finished display not found in log; staffing branch `night/vassal-redesign` redirected 11-20 | See the 11-20 row above |
| Vassal roads: trunk routes linking production, centre, gates; bridges first | 11-05, 11-06, 11-09 x3 | PARTIAL | #693, #721 (roads, a bridge), #750 budget | Trunk-road planner and bridge priority |
| Vassal calls Generals from too far (5 across a river) | 11-09 x1 | PLANNED | #688 uses 100-tile radius; owner says too large | Shrink radius and require reachable path |
| What Vassals do with their 20 defence units | 11-09 x1 | PLANNED | Unclear to owner; no PR | Document and show in UI |
| Steward and Vassal zones in walking distance (1 s recompute; whole resource patch belongs to the zone) | 11-05, 11-09 x2 | PARTIAL | #704 stays inside walkable territory | Walking-distance zones, patch ownership |
| Management hierarchy: office engine, provinces from geodata, Army/Economy/Navy/Church/Intel/Works/Expansion tracks, intents down and reports up | 10-31, 11-03, 11-08 x3, owner: "later" | PLANNED | Design discussed with the owner 10-31 (office tracks, provinces, intents); not written in repo docs; no code | First step when owner says go: one generic office engine plus province generation |
| Sergeants: one hero per type (infantry 20, archers 15, cavalry 10, siege 5), General leads 10 squadrons | 11-03 x1 | PARTIAL | #668, #689 (typed squads and rosters); hero assets for sergeants not found | Check after the hierarchy decision |
| Province map as far view; LLM counsel as top manager; light supply far from own provinces | 10-31 | PLANNED | Open question to owner, unanswered | Later |
| Vassals and Generals desync/override owner orders | 11-03, 11-09 | PARTIAL | #711, #800 | Verify on real terrain |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| The General: Castle leader, 50-tile sphere, Captain/Trainer modes, mix sliders | 10-29, 10-30 | DONE | #633, #662 |
| Generals recruit only on creation; owner order takes soldier out | 11-09 | DONE | #800, #711 |
| Steward takes over TC economy; Vassal defends and fortifies land | 11-03 | DONE | #685, #688 |
| Vassal stability: churn, farm tiling, adoption, split overlapping territory | 10-30, 11-03 | DONE | #641, #648, #680 |
| Vassal "too many units nearby" census stall; Stewards delete old lumber camps | 11-10, 11-09 | DONE | #762 |
| Steward stuck villagers recovered, farms when food dry | 11-05 | DONE | #704 |
| Walled Steward-sized domain with roads and a bridge | 11-05 | DONE | #721 |
