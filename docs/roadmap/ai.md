# Roadmap: AI opponents

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
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
