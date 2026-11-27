# Game score: the choke scenarios

The owner asked for the game score to measure "the ability to cross chokes" and "fights",
on real maps that are really in the game. `natural` and `late` (see [game score](game-score.md))
play whatever the AI decides: in them the armies meet at a choke or they do not. `march` and
`battle` stage it on purpose, so it always happens, on the real map, at one real choke.

```sh
make game-score GAME_SCORE_ARGS='--scenario march --workers 1,4'
make game-score GAME_SCORE_ARGS='--scenario battle --workers 1,4'
make game-score GAME_SCORE_ARGS='--survey-chokes --survey-at 1213,1395'   # how the choke was found
```

## What is staged

No village, no unit, no AI seat: the package is loaded exactly as a match loads it (page
residency, prepared navigation, the clock rate; `HeadlessMatch::bare_world`), the units are
put on the open ground around two anchors, and each army is sent across as one formation move,
the order a player gives. Then the world is ticked and read, like any scenario.

**No AI seat, on purpose.** With seats played by the Rust AI the General may redirect an army
and the Steward may pull villagers into the neck, and the number would measure what the AI
decided to do that day. The AI is what `natural` and `late` measure. A staged scenario with
no AI is also simpler and exact: the only input is the layout, and the seed shuffles it.

| scenario | armies | orders | ticks | window |
| --- | --- | --- | --- | --- |
| `march` | seat 0: 800 Militia on the near bank | one formation move (a Line, 56 abreast) to the far anchor | 2,400 (4 game minutes) | `crossing`, the whole run |
| `battle` | seats 0 and 1: 400 each (160 Militia, 80 Spearman, 110 Archer, 50 Skirmisher), on opposite banks | one formation attack-move each to the other army's anchor, so that they meet in the ford | 4,500 (7.5 game minutes) | `fight`, the whole run |

Both armies stand on the open ground around their anchor (the nearest walkable tiles, one unit to a
tile), at least eight tiles from the line across the ford and so clear of it and of each other.

## The choke

**The ford at 1213,1395** (tile coordinates of the 2,000-tile map; the record is
`game_score/scenarios/anatolia.rs`, and `--survey-at 1213,1395` measures it again):

| | |
| --- | --- |
| what it is | a ford: five tiles of shallows (class `,`, wet ground a unit can walk) in a river (class `~`, not walkable) that runs north to south past it for more than sixty tiles |
| cross-section | x = 1213, from the river tile at y = 1391 to the river tile at y = 1397: the five walkable tiles between them |
| width, length | 5 tiles wide; three cross-sections in a row no wider than seven tiles: about three tiles of water to wade |
| the march | west to east |
| what blocks the sides | river on both sides: all seven wall tiles read along the neck are river |
| why it qualifies | the survey finds it as a saddle, the narrowest point of the widest way between 1,084 and 24,508 tiles of open ground (clearance 2.0 tiles). Closing its five tiles lengthens the walk between anchors 30 tiles either side from 62 to 114 steps (1.84 times), between anchors 50 tiles either side from 117 to 157: the next crossing of the river is the way around |
| anchors | the near army around (1182,1393), the far army around (1243,1394) |

It is a ford, not a bridge: a bridge is built by a player, and the owner's wish was for real
crossings of the real map. A bridge is the narrower choke (one tile, the `long-bridges` fixture of
the simulation) and the sharper test of the same numbers, if the owner wants it.

## How it was found

`make game-score GAME_SCORE_ARGS='--survey-chokes'` reads the walkable tiles of the map, the
same answer the route planner gives (`route_walkable`: the map's own passability, trees and other
resource nodes, bridges), and does what a watershed does:

1. **Clearance.** Every walkable tile gets its distance to the nearest tile that blocks, in
   thirds of a tile.
2. **Saddles.** The tiles are taken from the widest to the narrowest and each joins the ground
   around it through its four neighbours (a gap that only touches at a corner is no way through
   for an army). A tile that joins two stretches of ground, each of at least 400 tiles of
   clearance two or more, is a saddle: the narrowest point of the widest way between them.
3. **The neck.** At the saddle the horizontal and the vertical run of walkable tiles are measured;
   the shorter is the cross-section (the tile that blocks at each end, the walkable tiles between
   them are the width), and its perpendicular is the march. The length is the number of
   cross-sections in a row, both ways, no wider than the narrowest plus two tiles.
4. **A neck an army has to use.** Two anchors are placed 30 tiles either side on open ground. The
   way between them is walked (four neighbours) with and without the cross-section. A neck is kept
   when closing it makes the way 1.15 times longer or shuts it.

On the owner's map: 455 saddles, 441 distinct, 137 kept (0.5 s). The survey is the data; the
choice among them is a judgement, written down with its reasons: a river ford (one of the three
kinds of crossing the owner named) with open land on both banks that holds a few hundred units each
and a detour long enough that the army has to use it. The strongest detours on the list are
coastal strips and forest lanes between a sea or a wood and a wall, whose near sides are too narrow
to stand a few hundred units on (the first try, the coastal pass at 644,1572, placed 56 of 400). `--survey-at x,y` measures any tile again.

The staging checks the record against the map it plays on, at the start of every run: the
survey's own measure of the neck at the recorded saddle must give the recorded cross-section,
width, length and march, the two tiles at the ends must block, the tiles between them and both
anchors must be walkable. A map that changed (another hash is a staged scenario on another map: it
is left out of the default set and refused by name when asked for) or a record copied wrongly stops
the run with the difference in words.

## The numbers

Under `service.choke` in every window of a staged scenario, and `service.battle` when two armies
are staged (the report's schema stays 1: a reader that does not know them does not see them in
a scenario without them). Ticks are those of the run (ten ticks are one game second).

| number | how it is measured |
| --- | --- |
| `units_sent` | units staged |
| `crossed` | units that crossed the line for the first time in the window. The line is the segment between the centres of the two tiles that block the cross-section; a unit has crossed when one step of it cuts that segment (once; going around an end is not a crossing, the ends are walls). In `battle` each unit crosses from its own side |
| `crossed_per_minute_peak` | the most units that crossed in any 600 ticks of the window: what the neck lets through. The per-minute entries of `series` hold the crossings of each game minute |
| `first_crossing_tick`, `last_crossing_tick` | the first and the last tick a unit crossed |
| `waiting_near_side` | units alive at the end that have not crossed |
| `standing_still_inside_ticks` | the longest a unit stood still (position unchanged) inside the neck: between the two ends and within half the length plus one tile of the line |
| `arrived_at_goal`, `all_arrived` | units alive within two tiles of the last goal they were given at the end, and whether that is every unit sent. A soldier of a formation has the slot it was assigned as its goal |
| `battle.first_blow_tick` | the tick of the first hit |
| `battle.fought`, `battle.queued` | units that struck or were struck for the first time in the window, and units that had done neither by its end: the ones that only queued behind (who hit whom is the engine's damage observation, which it never reads back) |
| `battle.fight_ended_tick` | the first tick with no unit of one army alive, if there was one |
| `battle.alive_seat_0_permille`, `..._1_...` | the share of each army alive at the end |

Besides these, the window has the numbers every scenario has: arrival, route planning (searches,
stuck, waiting, wait p95), hits, first blows and the ticks they waited, kills and losses.

## What the first run shows

Seed 1, at 1 and at 4 workers: the same game, the same hash every ten ticks, the same orders, the
same numbers. On the OrbStack machine of the owner's Mac, through `make game-score`, with other
agents' builds running.

| | `march` | `battle` |
| --- | --- | --- |
| native seconds at 1 / 4 workers (of which the start) | 1.7 / 1.5 (0.9 / 0.4) | 9.5 / 16.4 (0.5 / 1.0) |
| units across the line | 800 of 800 | 323 of 800 |
| first / last unit across (tick) | 193 / 1,036 | 293 / 2,869 |
| across per game minute | 534, 266 | 160, 84, 62, 15, 2 |
| most across in one game minute | 776 | 241 |
| waiting on the near side at the end | 0 | 111 |
| longest stand still inside the neck | 1,738 ticks | 634 ticks |
| within two tiles of their goal at the end | 796 of 800 | 5 of 800 |
| hits, kills | none | 10,411 and 689 |
| first blow, units that fought / only queued behind | none | tick 285, 755 / 45 |
| alive at the end | all | 15% of seat 0, 13% of seat 1 |

On a quiet machine the same runs took 1.3 / 1.4 s and 5.6 / 5.7 s (at 4 workers a busy machine
can be slower than at 1, as the 16.4 s shows). Facts, not findings to fix: the army of 800 crosses
the five-tile ford in 84 seconds, at up to 776 units in a game minute (13 a second). A unit stands
still inside the ford's reach for 1,738 ticks, almost three game minutes: a diagnostic run found
nine units standing within two tiles of the ford's exit for the rest of the run, the slot each was
last assigned the tile it stands on, which is why "within two tiles of their goal" (796 of 800)
does not see them. In the battle 755 of 800 units strike or are struck and 45 only queue behind;
689 die (deaths per game minute: 77, 243, 158, 56, 42, 44, 49, 20); the fight does not end: 111
units survive, every one on the bank it started on, and only 323 units ever cross the line.

## The seed and the usual spread

The seed deals the kinds and the places out in another order (the places are the nearest tiles of
open ground around the anchor, one unit to a tile): who stands where, and so which unit has which
id, the engine's tie-break, is the layout. Seed 1 is the default and the same seed is the same game.

What a layout changes is small in `march` and large in `battle`. In `march` every unit is a
Militia, the army moves as one formation, and eight layouts give the same crossing to the tick:
the same 800 across, the same 776 in the best minute, the same first (193) and last (1,036) unit
across, the same stand still inside the neck; only the units within two tiles of their goal move
(794 to 800). So a move of the choke numbers of `march` is not seed noise, and its tolerance is the
verdict's floor (5 percent, and 10 units or 3 ticks). In `battle` the layout decides who meets whom
first, and the fight is chaotic. Measured on 2026-11-27 at `5c5222ff`, seeds 1 to 8, one worker
(the numbers `march` and `battle` have; the rest of what the verdict judges is zero or flat in them:
resources delivered, idle villagers, route orders stuck or expired, movers waiting for a route):

| scenario | number | median | spread (MAD / median) | range | tolerance |
|---|---|---|---|---|---|
| march | share of orders arrived | 79.8% | 2.5% | 74.4% to 88.6% | 5% and 14.2% |
| march | order to arrival, p95 (ticks) | 202.5 | 1.2% | 200 to 236 | 5% and 36 |
| march | units across the line | 800 | 0.0% | 800 to 800 | 5% and 10 |
| march | most across the line in a game minute | 776 | 0.0% | 776 to 776 | 5% and 10 |
| march | first unit across (tick) | 193 | 0.0% | 193 to 193 | 5% and 10 |
| march | last unit across (tick) | 1,036 | 0.0% | 1,036 to 1,036 | 5% and 10 |
| march | longest stand still inside the neck (ticks) | 1,738 | 0.0% | 1,738 to 1,738 | 5% and 3 |
| march | units still waiting on the near side | 0 | 0.0% | 0 to 0 | 5% and 10 |
| march | units within two tiles of their goal | 796 | 0.1% | 794 to 800 | 5% and 10 |
| battle | share of orders arrived | 39.0% | 5.4% | 30.5% to 42.1% | 11% and 11.6% |
| battle | first blow (tick) | 286 | 1.0% | 274 to 291 | 5% and 17 |
| battle | units that only queued behind | 25 | 18.0% | 8 to 45 | 36% and 37 |
| battle | units across the line | 325 | 4.2% | 288 to 343 | 8% and 55 |
| battle | most across the line in a game minute | 244.5 | 2.0% | 214 to 253 | 5% and 39 |
| battle | first unit across (tick) | 293 | 1.2% | 283 to 298 | 5% and 15 |
| battle | last unit across (tick) | 2,540 | 12.3% | 2,015 to 4,493 | 25% and 2,478 |
| battle | longest stand still inside the neck (ticks) | 846 | 9.3% | 617 to 940 | 19% and 323 |
| battle | units still waiting on the near side | 89 | 21.9% | 68 to 124 | 44% and 56 |
| battle | units within two tiles of their goal | 8.5 | 17.6% | 5 to 13 | 35% and 10 |

The whole table, every judged number of every window, is
[`scenarios/spread.json`](../../crates/harness/src/game_score/scenarios/spread.json); a test fails
when a scenario, a window or a judged number has no entry.
