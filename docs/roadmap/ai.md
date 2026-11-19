# Roadmap: AI opponents

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| AI economy stalls late game: 69-74% of the big AIs' villagers idle, population falling, gold-starved | 11-19 x1 | PLANNED | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s): P41 1,732 of 2,494 villagers idle, P3 880 of 1,283, owner 27 of 721; units 61,837 to 61,136 in 10 min. AIs hold 200-560k food and wood under 1,500 gold. Code: the Python AI leaves Steward-managed villagers to the Steward (`defer_to_steward`); `STEWARD_RULES.domains_per_pass` 8 is world-wide, so with 392 Stewards each is visited about every 49 game s with 8 resource searches; the AI Market sells 100 per 8 s | Measure the idle cause on the save first; then per-player Steward passes, Market trade scaled to the surplus |
| AI armies stay where they were trained; they do not mass and counter-attack | 11-19 x1 | PLANNED | Owner playtest 11-19 (Anatolia 2, 10 Hz build, 61.5k units, 4.0 ticks/s): P41 3,916 of 3,952 soldiers idle; P3 (strongest) cannot answer the owner's attack; AI attack approaches wait p50 354 ticks for a route | Rally armies to a staging point, then mass and attack; check route waits per AI |
| Much stronger scaling AIs | 11-06, 11-12, 11-13 x3 | PARTIAL | #803 (median 120-min pop 292 to 716), #807, #820 (plain start 29 to 876 pop). Live 11-15 min 75: median AI military 20, 19 of 89 AIs with no army, best AI pop 255-371 | Brief a duel/tournament batch on real maps (`ai/win-real-maps`, 5 commits, in flight; its 48-game batch was paused while the owner plays) |
| AIs upgrade their units and use new content | 11-15 x1 | PLANNED | Measured on 38 Imperial AIs: only 6-19% of each line upgraded, 5-7 AIs upgrade properly; AIs train only pre-content units. Hypothesis: gold found early, little market use | Log per-AI market trades and gold income, then teach AIs the new units (only after content merges) |
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
