# Long-game balance baseline on France

Both matches produced Imperial realms with five Town Centers. All 102 seats
with a sighting had one TC at first contact, with median times of 21.18
minutes at eight seats and 8.06 at 100. Late food and gold availability
varied sharply by realm. At 100 seats, 17 finished with under 100 banked
gold even though the median seat still had 56.3% of its home ore.

Two fresh Dark Age matches used the default in-process Rust AI and standard
village setup. The map was the owner's source-backed France package from
the 11-25 playtest, 2,000 × 2,000 tiles at 240:1, recipe 4,
`931bccc765331e935890b59b154604742ed43e04cfd1fcb25ee7cc0059b7a1cf`.
Both used village seed 1, four workers and a 10 Hz game clock. Seat count
changes the village layout and density on this fixed map. The engine and
observer revision was `d09b7ba317c1d00ee187f28d6b2620b78a738f48`.

```sh
nice -n 10 make balance-match BALANCE_MATCH_ARGS='--seats 8,100 --seed 1 --workers 4 --game-minutes 120 --wall-minutes 90 --package-directory .cache/balance/packages --map-hash 931bccc765331e935890b59b154604742ed43e04cfd1fcb25ee7cc0059b7a1cf --out .cache/balance/france'
```

| Seats | End tick | Game minutes | Wall minutes | Partial starting kits | Stop |
| --- | ---: | ---: | ---: | ---: | --- |
| 8 | 72,000 | 120 | 13.26 | 0 | Tick target |
| 100 | 55,251 | 92.085 | 90.00 | 0 | Wall limit |

The local evidence is `.cache/balance/france/{8,100}-seats.jsonl`.
Stream validation covered 126 samples at eight seats and 178 at 100.
Every seat has a minute-by-minute census and first-sighting record if
observed. The streams also retain cumulative deliveries and exact
completion ticks for every age and completed technology. Absent events
are right-censored at the final tick. All cross-seat medians below
include all seats at that checkpoint, including defeated seats.
Milestone timing medians include seats that reached the milestone.
Clock rules and field definitions are in the [measurement guide](README.md).

## Contact and progression

First contact is a line-of-sight sighting. An enemy scout or public building
qualifies, and first sighting can occur at a different time for each of two
opponents. Time is in game minutes.

| Seats | Seats with contact | First sighting min / median / max | TCs at contact min / median / max | Techs at contact min / median / max | Army at contact min / median / max |
| --- | ---: | --- | --- | --- | --- |
| 8 | 6/8 | 16.38 / 21.18 / 28.90 | 1 / 1 / 1 | 2 / 2 / 3 | 1 / 1.5 / 16 |
| 100 | 96/100 | 2.65 / 8.06 / 36.49 | 1 / 1 / 1 | 0 / 1 / 14 | 1 / 1 / 44 |

| Seats | Feudal count / median min | Castle count / median min | Imperial count / median min | Five TCs count / median min | Six TCs count / median min |
| --- | --- | --- | --- | --- | --- |
| 8 | 8 / 12.93 | 6 / 25.25 | 3 / 41.69 | 1 / 116.84 | 0 / — |
| 100 | 98 / 12.89 | 87 / 24.40 | 60 / 51.91 | 2 / 83.38 | 0 / — |

Five of the six observed eight-seat contacts occurred in Feudal and one in
Castle. The two seats without sightings finished in Feudal with one Town
Center and two technologies each. Seat 5, the one that built five TCs,
had already seen an opponent at minute 20.79 and reached Imperial at 38.41.
In the 100-seat game, 70 first contacts occurred in Dark Age, 16 in Feudal
and 10 in Castle. The two five-TC seats first saw an opponent at minutes
21.77 and 16.47 and completed their fifth TCs at 77.81 and 88.96,
respectively. Each of the four seats without a sighting finished with
one TC and at most nine technologies.

## Income, stockpiles and gold

Deliveries are gross credited harvests per game minute over the named window.
Stocks are banked resources at the end of the window, after all sources and
spending. Resource order is food, wood, gold, stone. Home gold covers the
same natural and starting-kit piles near the original TC throughout.

| Seats | Game-minute window | Median food / wood / gold / stone delivered per minute |
| --- | --- | --- |
| 8 | 0–30 | 152 / 48 / 45 / 0 |
| 8 | 30–60 | 85 / 54 / 47 / 0 |
| 8 | 60–90 | 72 / 61 / 43 / 0 |
| 8 | 90–120 | 81 / 136 / 64 / 4 |
| 100 | 0–30 | 142 / 101 / 47 / 0 |
| 100 | 30–60 | 131 / 171 / 156 / 5 |
| 100 | 60–90 | 160 / 203 / 152 / 0 |

In the eight-seat game, seat 7 averaged 1,111 food and 2,110 wood
deliveries per minute over minutes 90–120. Its food bank increased by
1,595 over those thirty minutes. In the 100-seat game, the median
food bank was 88 at minute 90 even with 160 food delivered per minute
during the preceding thirty minutes.

| Seats | Game minute | Median food / wood / gold / stone banked | Median villagers / army | Median TCs / finished research | Median home gold remaining |
| --- | ---: | --- | --- | --- | ---: |
| 8 | 30 | 934 / 56 / 725 / 200 | 30 / 12.5 | 1 / 3 | 94.3% |
| 8 | 60 | 133 / 128 / 478 / 200 | 41 / 15 | 1 / 13 | 87.7% |
| 8 | 90 | 389 / 167 / 1,273 / 200 | 46.5 / 15.5 | 1 / 14.5 | 85.1% |
| 8 | 120 | 1,388 / 128 / 1,598 / 200 | 40.5 / 36 | 1 / 17 | 80.0% |
| 100 | 30 | 36 / 62 / 438 / 200 | 33 / 22 | 1 / 7 | 93.6% |
| 100 | 60 | 110 / 58 / 926 / 200 | 43 / 25.5 | 2 / 12 | 72.1% |
| 100 | 90 | 88 / 50 / 626 / 200 | 49 / 36.5 | 2 / 17 | 56.3% |
| 100 | 92.085 | 133 / 58 / 590 / 200 | 51 / 40 | 2 / 17.5 | 56.3% |

The eight starts each had seven home piles totalling 22,400 gold, with 3,200
per pile under the current rules. By minute 120, two seats had emptied
their surveyed gold, while the median seat still had 17,924 in the ground.
Seat 5 finished with 11 gold in the bank and seat 0 with 21,095.
Seat 5 had 18,999 home gold left and zero gold deliveries over minutes
90–120. At 100 seats the median initial home reserve was also 22,400,
with a range of 22,400–28,800. The median ended at 12,608 and the lowest
at 191. Forty-four seats ended below 500 banked gold. This is evidence
of difficulty keeping a spendable gold reserve on some seats, even with
the larger piles. The survey covers local piles only, and deliveries
omit Market proceeds, trade and relic income.

## Research prices and pace

The catalog in the run contains 105 technologies including age-ups. Their
listed prices total 26,530 food, 7,480 wood, 17,205 gold and 150 stone,
with 5,856 seconds of queue time summed across technologies. Several
buildings can research in parallel. The total is a price reference for the
catalog, and the match traces show which portion the bots actually bought
and finished.

Seat 7 in the eight-seat run finished 93 technologies. The listed cost of
those completions was 24,430 food and 12,420 gold, equivalent to 41% of its
59,391 food deliveries and 90% of its 13,794 gold deliveries. Those ratios
compare listed prices with credited harvests. They exclude other revenue
and any cancelled orders. Seat 7 reached Imperial at minute 41.69 and
its 93rd completion at minute 118.48. Seat 5 finished with 27 technologies
despite reaching Imperial earlier and building two more Town Centers.
At 100 seats, the leader completed 53 technologies at a listed cost
of 10,405 food and 7,490 gold. That is 35% of its 29,876 food deliveries
and 30% of its 25,166 gold deliveries. No seat completed the full catalog.

| Technology | Listed price | Queue seconds | 8 seats, count / median completion min | 100 seats, count / median completion min |
| --- | --- | ---: | --- | --- |
| Loom | 50 gold | 25 | 8 / 4.04 | 100 / 3.97 |
| Wheelbarrow | 175 food, 50 wood | 75 | 1 / 101.46 | 2 / 45.95 |
| Imperial Age | 1,000 food, 800 gold | 190 | 3 / 41.69 | 60 / 51.91 |
| Chemistry | 300 food, 200 gold | 100 | 3 / 58.52 | 46 / 59.10 |
| Conscription | 150 food, 150 gold | 60 | 1 / 108.53 | 1 / 86.33 |

Loom was affordable early. Cheap catalog entries also sat unresearched
for much of the game: Wheelbarrow cost 175 food and 50 wood and had
three completions across 108 seats. The traces establish the combined
effect of prices, AI priorities, prerequisites and production queues.

## What food upkeep would have to cost

Apply a hypothetical rate to each seat's observed army-minutes, estimated
by trapezoid integration between samples. For each seat and window,
a 25% delivery tax rate is `0.25 × food delivered / army-minutes`.
The rate needed to offset a growing bank is
`positive change in food stockpile / army-minutes`.
The first calculation scales with gross harvests. The second captures
surplus after observed spending and other credits. Both hold the observed
army and choices fixed. Seats with zero army-minutes have no defined rate.

| Seat and interval | Mean army | Food delivered/min | Food bank change | Food/unit/min to absorb 25% of deliveries | Food/unit/min to offset bank growth |
| --- | ---: | ---: | ---: | ---: | ---: |
| 8 seats, seat 5, min 90–120 | 329.2 | 629.5 | −389 | 0.48 | 0 |
| 8 seats, seat 6, min 90–120 | 214.0 | 227.3 | +1,250 | 0.27 | 0.19 |
| 8 seats, seat 7, min 90–120 | 362.9 | 1,110.5 | +1,595 | 0.76 | 0.15 |
| 100 seats, seat 1, min 60–90 | 94.8 | 529.3 | +185 | 1.40 | 0.07 |
| 100 seats, seat 97, min 60–90 | 92.5 | 500.7 | −228 | 1.35 | 0 |

At one food per unit per game minute, the seat 7 example would owe about
363 food per minute, or 33% of its measured food deliveries. Its observed
bank grew by only about 53 food per minute. Seat 5 was already drawing
down its food bank. A charge in this range competes with late-game
production on those economies. The median rate absorbing a quarter of
food deliveries over minutes 60–90 was 0.73 in the eight-seat run and
1.25 in the 100-seat run, among the seven and 87 seats with army-minutes.
The early sighting medians were 1.5 and one military unit. One food per
unit per minute would charge only that much at first sighting, consistent
with limited leverage over the pre-contact economy.

The upkeep estimates are arithmetic on the observed trajectories. They
give the rates that would change the food budget at the measured army
sizes. A trial with upkeep can measure how the AI changes army production,
harvesting and age-up choices.

## Limits

Each size is one seed on a fixed map with default Rust AI and fog.
Line-of-sight contact can precede combat. The owner observed a human
realm in their playtest. These games establish the AI baseline on the
same map. At 100 seats the final observation was minute 92.085, so the
minute-120 result remains unobserved at that scale. The second game
included later-Imperial and five-TC states within the wall budget. These
headless runs share the development machine and measure game pacing.
