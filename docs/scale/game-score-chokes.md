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
same numbers. On the OrbStack machine of the owner's Mac, `make game-score`.

| | `march` | `battle` |
| --- | --- | --- |
| native seconds at 1 / 4 workers (of which the start) | 1.3 / 1.4 (0.5) | 5.6 / 5.7 (0.8 / 0.4) |
| units across the line | 800 of 800 | 293 of 800 |
| first / last unit across (tick) | 193 / 1,036 | 293 / 4,427 |
| across per game minute | 534, 266 | 85, 168, 21, 6, 2, 3, 5, 3 |
| most across in one game minute | 776 | 239 |
| waiting on the near side at the end | 0 | 112 |
| longest stand still inside the neck | 1,738 ticks | 850 ticks |
| within two tiles of their goal at the end | 798 of 800 | 8 of 800 |
| hits, kills | none | 10,546 and 687 |
| first blow, units that fought / only queued behind | none | tick 292, 770 / 30 |
| alive at the end | all | 19% of seat 0, 10% of seat 1 |

As facts, not as findings to fix: the army of 800 crosses the five-tile ford in 84 seconds, at up to
776 units in a game minute (13 a second); 2 of the 800 never reach their slot, and a unit stands
still inside the ford for 1,738 ticks, almost three game minutes. In the battle 770 of 800 units
strike or are struck and 30 only queue behind; 687 die; the fight does not end: the survivors,
112 of 113 of them on the bank they started on, go on trading hits across the river (deaths fall
from 263 in the second game minute to between 25 and 52 in each of the last five) and only 293
units ever cross the line.

## The seed

The seed shuffles which unit takes which place on the open ground around the anchor (a fill of
the nearest tiles, kinds dealt out by a seeded shuffle), so a seed is a layout and the same seed
is the same game. Seed 1 is the default. The spread across seeds 1 to 8 is in
`scenarios/spread.json` and in [the usual spread](game-score.md#the-usual-spread).
