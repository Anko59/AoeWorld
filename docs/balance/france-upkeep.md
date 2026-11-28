# Military food upkeep on France

The trial charges one food per living military unit per game minute. The
rate sits between the 0.73 and 1.25 food per unit per minute needed to
absorb a quarter of late deliveries in the [France baseline](france-baseline.md).
Fractional charges accrue each tick. A missed payment halts new military
recruitment and doubles the cooldown set by unit attacks until food returns.
The Rust AI assigns more workers to food, budgets a minute of upkeep before
recruiting, and keeps an additional reserve for idle soldiers outside emergency
defence.

## Setup

Both runs use the 2,000 × 2,000 source-backed France request and seed 1 from
#1165, four workers, default Rust AI, standard Dark Age starts, and a 10 Hz
game clock. Each run targets 120 game minutes with a 90-minute wall budget.

The current engine requires recipe 5. The regenerated package
`17a33e03eb4f2ac8503a24f5f259f105e23d02c02cd30e440d009e1aeed6d34e`
has the same request, projection, prepared environmental page roots and
source content digests as the recipe 4 package used in #1165. The current
recipe changes starting wildlife and fish stocks and enables recovery. The
underlying terrain and geographic seed are the same.

```sh
nice -n 10 make balance-match BALANCE_MATCH_ARGS='--seats 8,100 --seed 1 --workers 4 --game-minutes 120 --wall-minutes 90 --package-directory .cache/balance/packages-v5 --map-hash 17a33e03eb4f2ac8503a24f5f259f105e23d02c02cd30e440d009e1aeed6d34e --out .cache/balance/france-upkeep-v5'
```

The baseline engine and observer revision was
`d09b7ba317c1d00ee187f28d6b2620b78a738f48`. The trial ran at
`185091e4459429118631c0e82a4339a21b10aa6c`. The locally retained
JSONL streams include every complete minute, exact first sightings, and
the actual ending tick and reason.

These runs preceded the review fixes. The trial chose each cooldown when
the unit attacked. The final rule adjusts cooldown progress as supplies
change and clears a shortage as soon as food is credited.

## Eight seats

Both matches reached tick 72,000. The baseline took 13.26 wall minutes,
and the trial took 29.46 on the shared development machine.

| Metric | #1165 baseline | Upkeep trial |
| --- | ---: | ---: |
| Seats with contact | 6/8 | 7/8 |
| Median first sighting (game min) | 21.18 | 21.91 |
| Median Town Centers / techs at first sighting | 1 / 2 | 1 / 3 |
| Median delivered food/min, min 60–90 | 72 | 362 |
| Median banked food / army at min 90 | 389 / 15.5 | 129 / 84.5 |
| Median banked food / gold at min 120 | 1,388 / 1,598 | 736 / 619 |
| Median villagers / army at min 120 | 40.5 / 36 | 165 / 255.5 |
| Median Town Centers / finished research at min 120 | 1 / 17 | 2.5 / 75.5 |
| Seats reaching Imperial / five Town Centers | 3 / 1 | 5 / 3 |
| Most research finished by one seat | 93 | 100 |
| Median home gold remaining at min 120 | 80.0% | 9.0% |

At scheduled minute checkpoints, 122 of 960 seat observations showed a
shortage. The trial reached similar first-contact times and smaller late food
banks, with larger late kingdoms and armies on this seed.

### Eight seats with the same map recipe and engine

The branch parent `7efae024b9959b862dbbddd2ae348269419823b5` ran on the same
recipe 5 package with the same seed, clock, workers and stop criteria.
The control command selected `--seats 8` and wrote to
`.cache/balance/france-control`. It reached tick 72,000 in 38.18 wall minutes.

| Metric | Branch parent | Upkeep trial |
| --- | ---: | ---: |
| Median first sighting (game min) | 21.92 | 21.91 |
| Median banked food / army at min 60 | 115 / 83.5 | 39 / 22.5 |
| Median banked food / army at min 90 | 330.5 / 136 | 129 / 84.5 |
| Median banked food / army at min 120 | 825 / 457.5 | 736 / 255.5 |
| Median delivered food/min, min 60–90 | 317.1 | 362 |
| Median Town Centers / finished research at min 120 | 4 / 62 | 2.5 / 75.5 |
| Seats reaching Imperial / five Town Centers | 6 / 4 | 5 / 3 |

On this matched seed the upkeep trial had smaller armies and food banks at all
three checkpoints, with nearly the same first sighting time. The median seat
finished more research and held fewer Town Centers at minute 120.

## One hundred seats

Both runs reached the 90-minute wall limit. The baseline stopped at tick
55,251 (92.085 game minutes), and the trial stopped at tick 58,138
(96.897 game minutes).
Both columns use data through complete game minute 92.

| Metric | #1165 baseline | Upkeep trial |
| --- | ---: | ---: |
| Seats with contact | 96/100 | 97/100 |
| Median first sighting (game min) | 8.06 | 8.15 |
| Median Town Centers / techs at first sighting | 1 / 1 | 1 / 1 |
| Median delivered food/min, min 60–90 | 160.15 | 337.43 |
| Median banked food / gold at min 92 | 134 / 617 | 230.5 / 459.5 |
| Median villagers / army at min 92 | 51 / 39 | 77.5 / 34 |
| Median Town Centers / finished research at min 92 | 2 / 17.5 | 3 / 22 |
| Seats reaching Imperial / five Town Centers | 60 / 2 | 70 / 31 |
| Most research finished by one seat | 53 | 99 |
| Median home gold remaining at min 92 | 56.3% | 44.7% |

At scheduled minute checkpoints through minute 92, 133 of 9,200 seat
observations showed a shortage, across 63 seats. Compared with #1165, the
trial had a smaller median army at this checkpoint and higher median food
banks, deliveries and completed research.

## Limits

The comparison uses common observed game minutes. Medians include all seats
at each checkpoint, including defeated seats. Income measures gross credited
harvests. Stockpiles reflect all sources and spending. Shortage counts sample
whole game minutes and can miss brief episodes.

Intervening changes to resource stocks, regeneration, movement and AI can
also affect the trajectories. Each seat count uses one seed. The wall times
reflect the workload and the shared machine.
