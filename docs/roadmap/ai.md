# Roadmap: AI opponents

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Late-game AI villagers stay idle and gold runs short | 11-19 x1 | IN FLIGHT | In the owner playtest, 69–74% of the big AIs' villagers were idle and units fell from 61,837 to 61,136 in 10 min. Four of 13,229 idle AI villagers were Steward-managed at the end of the restored baseline. Rotating worker assignments and briefly surveying large Castle and Imperial settlements cut idle workers from 13,229 to 11,574 at nearby game ticks, with route waits rising from 5,079 to 6,857. Market sale orders carried 460 lots in the first 180 ticks, up from 51. | Measure completed trips and sustained gold income over a longer game. |
| AI armies stay near their training buildings | 11-19 x1 | IN FLIGHT | Owner playtest #907 (Anatolia 2, 61.5k units): P41 had 3,916 idle soldiers out of 3,952 and AI attack approaches waited p50 354 ticks for a route. In a two-seat fixture with 16 militia split into two groups, the baseline launched immediately. Candidate `c09a17a5a` spent 40 game seconds assembling, then marched from its base. Both sent all 16 past the base boundary by 120 seconds. At 155 seconds, non-idle militia went from 11 to 10 and kills from 1 to 0. | Check combat outcomes at scale. |
| Much stronger scaling AIs | 11-06, 11-12, 11-13 x3 | PARTIAL | #803 (median 120-min pop 292 to 716), #807, #820 (plain start 29 to 876 pop). Live 11-15 min 75: median AI military 20, 19 of 89 AIs with no army, best AI pop 255-371 | Brief a duel/tournament batch on real maps (`ai/win-real-maps`, 5 commits, in flight; its 48-game batch was paused while the owner plays) |
| AIs upgrade their units and use new content | 11-15 x1 | PARTIAL | Owner playtest with 38 Imperial AIs: only 6–19% upgraded each line. Matched Imperial skirmish (seed 41, four AIs, 4,800 ticks): completed line upgrades 10 → 34 across 5 → 7 lines; military unit kinds 7 → 16, including Bombard Cannon, Camel Rider, Cavalry Archer, Scorpion and Trebuchet. Costs in the same run: total spawns 55 → 45, military recruits 46 → 41, seat 3 kinds 4 → 2, seat 4 trained nothing in either run | Find why total output fell and seat 4 trains nothing; measure on a long 99-AI game |
| Force projection across water and rivers, forward buildings | 11-13 x1 | IN FLIGHT | `feat/ai-projection` 1 commit, 3 known defects, needs rebase (PR 819 closed; backup branch kept) | Rebase, fix defects, prove on an island map |
| AIs must use Steward, Vassal, General; reach Imperial | 11-06 x1 | PARTIAL | #723 (five strategy drafts, cost-aware tournament). 11-06 note: drafts never fielded a Steward in a long game; default AI baseline | Check Steward use in the tournament report |
| Fast AI training mode (in-process bots, 100-500x for 1v1) | 10-22 "later" | PLANNED | Match-bench Rust bots exist (#740) but are a minimal tier, not a training mode | Only when the owner asks |
| AI cost: actions per minute, CPU, tick-rate effect per bot | 11-06 | PARTIAL | #834 order volume by issuer; tournament scoring in #723 | Fold into tournament report |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| Python AIs via a Rust SDK, multiplayer matches, standard starts | 10-21 | DONE | #551, #553 |
| 99 AIs at full speed, fan-out, scale-match | 10-22 | DONE | #556, #557, #559, #566 |
| AI hosts last the whole match (6 h limit killed all AIs) | 11-05 | DONE | #703 |
| Protect army wood, fund lumber camps, keep worker lanes | 11-05 | DONE | #690, #677 |
