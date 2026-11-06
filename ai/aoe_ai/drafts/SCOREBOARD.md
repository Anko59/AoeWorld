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

## Round 3: stalls, expansion and one long game

**Stall diagnosis** (6 baseline bots, seed 4, sampled every few minutes: stockpile, tasks, buildings, refusal counters):
the Feudal stalls were mostly *no building site*. `find_build_site` searched 18 tiles around the Town Center, so a
cramped start (river, cliff, forest) never placed its Archery Range or Blacksmith (one start sat on 1,062 wood and
892 food with only a Barracks) and so never met the two Castle requirements; another could not place Houses (99
refusals) and sat at its population cap. Fix: the search widens to 2x and 3x the radius and finally to gap 0; the same
run then had all three buildings by minute 20. Other causes seen and not fixed: gather orders refused as unreachable
on fish-and-island starts (wood 21 beside 1,598 food), and the Castle gate (`castle_villagers` hold plus
`castle_army_min`) that freezes villagers at 28 while a weak food income saves for 800.

**Expansion** (Town Centers need Castle Age here: 275 wood, 100 stone): drafts place extra Town Centers from Castle Age,
trying six directions around the village, keep the wood back for the next one and lean shares to wood; houses scale with
Town Centers; every Town Center keeps a villager queue; caps 150 to 400 villagers.

**Long game**: seed 5, package `9b2864a2`, all six drafts, release server, revision `66022121f`. Planned 120 game
minutes; machine load (average up to 65) held it to 44.2 game minutes in 3,290 s of wall time. CPU columns are inflated
by that load and not comparable with rounds 1 and 2.

| draft | games | wins | mean score | Imperial (games) | first Imperial min | mean peak pop | mean kills | mean losses | orders/min | CPU ms/decision | slowest decision ms | adjusted score |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| fast_castle | 1 | 1 | 861 | 1 | 44.0 | 111 | 0.0 | 0.0 | 38 | 72.6 | 1315 | 320 |
| steward_scaler | 1 | 0 | 809 | 0 | - | 259 | 0.0 | 0.0 | 46 | 198.0 | 6406 | -233 |
| baseline | 1 | 0 | 776 | 0 | - | 189 | 31.0 | 22.0 | 75 | 148.5 | 1823 | -68 |
| boom | 1 | 0 | 730 | 0 | - | 171 | 22.0 | 31.0 | 29 | 61.1 | 1532 | 236 |
| generals | 1 | 0 | 627 | 0 | - | 79 | 0.0 | 2.0 | 33 | 43.6 | 534 | 203 |
| balanced | 1 | 0 | 623 | 0 | - | 75 | 0.0 | 2.0 | 21 | 13.1 | 1169 | 621 |


Population and Town Centers every 10 minutes (D/F/C/I = age; S/V/G = Stewards/Vassals/Generals):

| minute | balanced | baseline | boom | fast_castle | generals | steward_scaler |
|---|---|---|---|---|---|---|
| 0 | D pop 4 TC 1 S0V0G0 | D pop 4 TC 1 S0V0G0 | D pop 4 TC 1 S0V0G0 | D pop 4 TC 1 S0V0G0 | D pop 4 TC 1 S0V0G0 | D pop 4 TC 1 S0V0G0 |
| 10 | D pop 21 TC 1 S0V0G0 | D pop 23 TC 1 S0V0G0 | D pop 27 TC 1 S0V0G0 | D pop 21 TC 1 S0V0G0 | D pop 21 TC 1 S0V0G0 | D pop 23 TC 1 S0V0G0 |
| 20 | F pop 30 TC 1 S0V0G0 | F pop 42 TC 1 S0V0G0 | C pop 35 TC 1 S0V0G0 | F pop 36 TC 1 S0V0G0 | F pop 32 TC 1 S0V0G0 | C pop 39 TC 2 S0V0G0 |
| 30 | C pop 44 TC 1 S0V0G0 | C pop 101 TC 1 S0V0G0 | C pop 58 TC 6 S0V0G0 | C pop 72 TC 3 S0V0G0 | C pop 47 TC 2 S0V0G0 | C pop 79 TC 7 S0V0G0 |
| 40 | C pop 66 TC 1 S0V0G0 | C pop 176 TC 1 S0V0G0 | C pop 149 TC 6 S0V0G0 | C pop 72 TC 3 S0V0G0 | C pop 79 TC 3 S0V0G0 | C pop 180 TC 10 S0V0G0 |


For comparison, the owner's real game at minute 126 had 1,649 population and 25 Town Centers.

First Imperial: `fast_castle` at minute 44.0 (Castle at 23). The best expander, `steward_scaler`, had 10 Town Centers and
259 population at the end, against 189 for the single-Town-Center baseline, but at 198 ms per decision under load.
No Steward, Vassal or General was fielded by anyone. `steward_scaler` hoarded 3,400 gold at minute 40 with food at 44:
every Town Center always had a villager queue, and a Steward needs an empty one and 1,000 food. The fix (save and
drain the queues and pause villagers for up to 240 s, then 180 s off) is committed after the long game and only
partly checked in play (see the report).

## Verdict

The default stays `baseline`: two games on one map, with start-position stalls that hit baseline in one of them, do
not establish that any draft beats it on strength and cost. On cost the drafts are cheaper (mean 10 to 31 ms per
decision against baseline's 58 and similar orders per minute). `generals` was the strongest in raw score,
`steward_scaler` the best adjusted for cost.
