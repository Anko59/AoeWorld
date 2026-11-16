# Roadmap: performance and the scale ladder

Ambition (owner 10-26, restated 11-07, 11-13): 1000 players and 10M units on this one machine (12 cores, 14 GB),
minimum 1000p/1M, real time at every rung, no queue (units never silently wait), level of detail allowed,
sharding later and never required. Near target 11-08: about 2M units for the management hierarchy.
Rungs: 0 measurement, 1 100k units, 2 500k, 3 2M, 4 clients (see issue 748).

| item | asked | status | evidence | next step |
| --- | --- | --- | --- | --- |
| Rung 1: owner Mekong save at 20 ticks/s and 30 FPS, then whole 100p/100k game | 10-22, 11-09, 11-12, 11-13 x12+ | PARTIAL | Owner save 7.4 (11-09) to 18.7 ticks/s (quiet, 11-10). Live game 11-15 Anatolia: 6.9 ticks/s at 16.7k units, 3.14 at game min 153 with one AI at 5k pop. War replay 391.5M to 251.3M instr/tick (#802 reference, #814 re-record, through #817); serial share 80.2%, Amdahl cap 1.25x | Cut work, not parallelise: plan_serve 48M, apply_loop 42M + group_apply 17M, combat 38M instr per tick; judge by `make save-replay` instructions AND the gated late_game counter |
| Client frame rate on large games | 11-09, 11-12, 11-15 | PARTIAL | #754 (2.8 to 16 FPS), #758 (14 to 43), #789, #792, #804, #816, #811 bench. Live 11-15: WebGL2 frame p50/p95/max 53/132/208 ms at 16.7k units | Profile the 11-15 save frame, `make client-frame-bench` |
| Rung 2: 500k units (movement rewrite core) | 11-07 | PLANNED | S1 kernel spike killed 11-14: 28,500 instr per mover vs gate 2,000; the plan's 1M at 20 Hz is not reachable with that design. Phase 4 and 5 parked (see [movement](movement.md)) | Owner decision: new design brief after current lag is fixed |
| Rung 1 bookkeeping leftovers: vision edge counters, event-driven transports, fog deltas on the wire, AI query API, ordered maps off the mover path, RSS per unit -30% | 11-09 | PLANNED | #744 #746 #747 #749 landed the main parts; none of these named leftovers found in the merge log | Pick by profile of the 11-15 save |
| Phase 7b rest: combat/training on timers, generational ids, gathering spikes | 11-09 | PARTIAL | #746, #773 (construction, queues, walking gatherers, idle units on timers); combat timers and ids not found in the log | After the lag profile |
| Phase 7a exact level of detail (analytic movers); rung 3 2M via hierarchy | 11-08 | PLANNED | No PR; needs the office engine ([hierarchy](hierarchy.md)) | Later |
| Phase 7c wire segments, 1,000 sessions, 128-seat cap lifted | 11-09 | PLANNED | No PR | Later |
| Phase 8 shards | 11-07 | PLANNED | Design must not block it | Later |
| Phase 0 leftovers: command recorder, Steward/General/Vassal bots in the closed loop, 30-minute 100-seat runs, France census with heights | 11-09 | PARTIAL | `make save-replay` (#802) replays recorded owner-save commands; match-bench #740; recorder for live matches not found | A live-match recorder lets every lag report be replayed |
| Tick-rate day 11-16: parallel movement apply, parallel combat, cheaper cold route-chunk builds | 11-16 | IN FLIGHT | Combat prefetch: 6-worker main-thread instructions 194.7M to 178.5M but +1.1% total, trimming. Sidestep backoff for blocked movers: #864. Movement slow path: write-buffer split then parallel decide. Cold portal-chunk build costs about 18M instr each | Ship each when main-thread instructions fall and gated counters stay within +1% |
| Tick cost of 99 Python AIs (about 3.5 ticks/s cost, 3.4 cores) | 11-04 | UNVERIFIED | Measured once 11-05, never profiled separately | Profile in match-bench |

## Done or dropped

| item | asked | status | evidence |
| --- | --- | --- | --- |
| Rung 0: owner-save benchmark, match-bench 1x-10x and 1,000 seats | 11-08 | DONE | #739, #740, #735 |
| Clearance pages, event indexes, dirty-diff fan-out, economy sleep, apply fast paths | 11-09 | DONE | #742, #747, #749, #744, #746, #752 |
| Save replay in instructions per tick; war replay reference re-recorded | 11-12 | DONE | #802, #814, #844 |
| 100 players / 99 AIs real time, France 16k tiles prep 4x faster | 10-22 | DONE | #556 to #559, #725, #726 |
| S1 kernel spike | 11-08 | DROPPED | Killed 11-14 by its own gates (parked in `parked/s1-kernel-2026-11-14`) |
| Phase 1b hot/cold unit split | 11-09 | DROPPED | +0.96% instructions, no gain on the save |
| Fan-out off the tick's path: overlapped delivery, cheaper frame | 11-16 | DONE | #858: live v6 save with 99 bots 9.69 to 11.75 ticks/s (single run, loaded machine), fanout_wait p50 12.4 to 5.3 ms |
| Parallel route serving (fair pass), rounds 2 to 4 | 11-16 | DROPPED | Main thread −9M instr/tick at 6 workers but late_game +8% to +23% from extra cold chunk builds; variant H parked on `park/plan-serve-h`, work moved to cheaper chunk builds |
| Formation-aware sidestep backoff for group members | 11-16 | DROPPED | Four rules tried; every one that backs off marching members broke real-terrain crowd recovery tests |
| Sleep, timers etc. "never built" claim | 11-12 | SUPERSEDED | Owner caught it: #746 and #773 shipped; see rework ledger in memory |
