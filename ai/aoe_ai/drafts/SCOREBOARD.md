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

## Latest run (seed 2, flat 400 tiles, 6 bots, 29.9 game minutes, stopped by the 1000 s wall limit)

| draft | age | Feudal / Castle min | peak pop | villagers | army | kills / lost | orders/min | CPU ms/dec | adjusted |
|---|---|---|---|---|---|---|---|---|---|
| boom | Castle | 13 / 23 | 100 | 55 | 45 | 0 / 1 | 23 | 11.4 | 649 |
| generals | Castle | 12 / 22 | 95 | 42 | 53 | 0 / 0 | 39 | 21.9 | 607 |
| steward_scaler | Castle | 13 / 24 | 94 | 52 | 42 | 0 / 0 | 35 | 12.0 | 644 |
| baseline | Castle | 12 / 21 | 90 | 33 | 58 | 1 / 6 | 53 | 52.6 | 175 |
| balanced | Castle | 13 / 23 | 80 | 53 | 27 | 0 / 0 | 22 | 11.3 | 630 |
| fast_castle | Castle | 13 / 22 | 62 | 40 | 22 | 0 / 2 | 25 | 11.0 | 610 |

No draft reached Imperial and no Steward, Vassal or General was fielded within 30 game
minutes (Castle Age arrives at minute 21 to 24); one seed only, so differences are within noise.
