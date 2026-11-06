# AI drafts and scoreboard

Each draft is a `Config` in `__init__.py`; `python -m aoe_ai host --draft NAME`
(or `AOE_AI_DRAFT=NAME`) plays one in a real match. Management units come from
`play/management.py`: Castles (stone) for Vassals and Generals, a Steward per
Town Center, extra Town Centers, and Generals that buy and march their army.

| draft | idea |
|---|---|
| baseline | the old default: boom into a Feudal army |
| boom | 110 villagers, 2 Town Centers, 1 Steward, 1 Castle with a Vassal, late army |
| fast_castle | early ages, 2 Castles each with a Vassal, knights |
| generals | Castle early, 2 Generals buying a cavalry-heavy army and marching it |
| steward_scaler | 3 Town Centers, 3 Stewards, workers left to the Stewards, up to 170 villagers |
| balanced | 2 TCs, Steward, 2 Castles, Vassal and a General in moderation |

Tournament: `python -m aoe_ai tournament --server-bin target/debug/aoe-server --seeds 2
--tiles 400 --minutes 40 --wall-limit 1000` (lockstep arena, drafts rotated over
slots by seed; admin `/admin/match` each game minute; evidence in `.cache/ai5/games`).
Cost columns: orders per game minute (`client.orders_sent`), thread CPU inside
`decide` per decision (the host runs all bots on one thread). Adjusted score
subtracts cost above half the limits (600 orders/min, 25 ms/decision) and 300 past a limit.
The server's `/admin/perf` is empty in the arena (no tick clock), so tick rate per draft
is not measured here.

## Round 2: release server, real map package, two games

Map package `9b2864a2...` (938 tiles, 30 km of real terrain), 6 bots, drafts rotated over slots
by seed. Seed 3 ran 36.2 game minutes (1094 s wall), seed 4 39.9 (994 s); both stopped by the wall
limit, not by the game. The machine was loaded (load average 10 to 22), so CPU figures are noisy.
Seed 3 ran before the "saving for management units" change, seed 4 after it.

| draft | games | wins | mean score | Imperial (games) | first Imperial min | mean peak pop | mean kills | mean losses | orders/min | CPU ms/decision | slowest decision ms | adjusted score |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| generals | 2 | 1 | 674 | 0 | - | 126 | 0.0 | 1.5 | 26 | 25.1 | 1953 | 473 |
| steward_scaler | 2 | 0 | 646 | 0 | - | 96 | 0.0 | 0.5 | 25 | 12.8 | 1014 | 638 |
| fast_castle | 2 | 0 | 632 | 0 | - | 82 | 0.0 | 0.5 | 48 | 27.8 | 884 | 420 |
| baseline | 2 | 1 | 532 | 0 | - | 82 | 0.0 | 0.5 | 32 | 58.0 | 2909 | 200 |
| balanced | 2 | 0 | 508 | 0 | - | 61 | 0.0 | 2.5 | 26 | 30.6 | 1527 | 286 |
| boom | 2 | 0 | 505 | 0 | - | 56 | 0.0 | 1.0 | 18 | 9.9 | 851 | 505 |


Per-game facts: Feudal at minute 12 to 14 in both games; Castle at minute 20 to 31 (seed 4: three of
six starts, including `baseline`, never left Feudal in 40 minutes, so the start, not the draft, stalled them).
Fielded in play: one General (`generals` draft, both seeds, minute about 28 to 33). No Steward and no Vassal was
ever trained: food, not the code path, is short (1,000 food per Steward). No draft reached Imperial. The Castle target
of minute 15 to 16 was not met: the best trace is Castle clicked at minute 19.7 (arrives about 22), because the start
(5 villagers, 10 food) gives 20 villagers at minute 8.

## Server tick cost, real time (release server, 6 bots of one draft, 330 s, 40 extra start villagers)

| draft | ticks/s | tick p50 / p95 / p99 ms | missed ticks | orders |
|---|---|---|---|---|
| generals | 19.98 | 2.4 / 5.7 / 10.6 | 6 | 1,919 |
| baseline | 19.97 | 2.7 / 7.0 / 11.2 | 10 | 1,823 |

Both are early-game (Dark and Feudal Age); neither shows a draft effect on the server, and nothing here says anything
about late-game armies. The arena (`tournament`) has no tick clock, so its tick records are only per-tick work.

## Verdict

The default stays `baseline`: two games on one map, with start-position stalls that hit baseline in one of them, do
not establish that any draft beats it on strength and cost. On cost the drafts are cheaper (mean 10 to 31 ms per
decision against baseline's 58 and similar orders per minute). `generals` was the strongest in raw score,
`steward_scaler` the best adjusted for cost.
