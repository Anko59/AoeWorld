# match-bench: the closed-loop in-process match (rung 0)

Movement rework phase 0 ([delivery plan](../movement/pathfinding-rework-delivery.md) §2, §5).
`make match-bench` plays the owner's late game of 2026-11-06 in one process, without a server
or Python hosts, and reports what breaks first as units grow.

- **World.** A procedural France map (`perf_scale::world::package`), one village per seat on a
  grid, then `GameWorld::seed_late_game` with the owner's seat shapes
  (`LateGameSeat::owner_human` / `owner_ai`, shared with `scale-match --late-game`) times
  `--scale`: units, farms, houses and camps times the factor on `sqrt(factor)` times the radius.
  The map side keeps the owner's density (2,000 tiles at 100 seats and 1x, growing with
  `sqrt(seats x scale)`) unless `--side-tiles` is given. 1x at 100 seats is about 9,700 units.
- **Seats above 128.** The world takes `WorldLimits::for_match(seats)`: the simulation's seat
  storage follows the match. The 128-seat `MAX_ECONOMY_PLAYERS` is the wire's (phase 7c).
- **Bots** (`match_bench/bots.rs`, closed loop): every second idle villagers return to the nearest
  resource; seats under their starting count train a villager and soldiers every 10 s; each AI seat
  sends its idle land army as one Line attack-move at a neighbouring seat every 60 s; slot 0
  splits its army into four Line attack-moves 300-700 tiles away every 45 s, and its Generals
  march. Each seat's main Town Center gets a garrisoned Steward. Vassals are not used.
- **Determinism.** `--threads 1,2,6,12` replays the same match per worker count and compares the
  canonical hash every `--hash-every` ticks; `--compare a.json,b.json` compares saved reports
  (runs in separate processes). Unequal sequence lengths fail comparison, even when the
  shared prefix agrees; the first unmatched tick is reported.
- **Instructions.** `--callgrind` re-runs the binary under Callgrind (cache simulation, one
  worker): `--warmup` ticks uninstrumented, then `--counted` ticks collected only inside
  `counted_tick`. The profile is split into the stages of `perf-stage-counters` plus whole tick
  phases and a `bookkeeping` sum (index relocation, vision, collect_changed, building memory).
  Simulated LL misses x 64 B stand in for DRAM bytes: `perf_event_paranoid` is 4 here.
  Cached profiles bind every option, the selected scale and the executable's contents;
  only a successful Callgrind process leaves a reusable completion marker.

Stewards spawn on the first passable tile around the main Town Center, so they can enter
without a route from a fixed offset. Setup fails with the seat and error if a required
Steward cannot be spawned or ordered inside. Earlier measurements below predate this check: their missing
Stewards mean they do not qualify the documented one-Steward-per-seat workload. They have
not been remeasured after the review fixes.

```sh
MATCH_BENCH_ARGS="--ticks 2000 --threads 6 --scale 1,5,10" make match-bench
MATCH_BENCH_ARGS="--seats 1000 --ticks 2000" make match-bench
MATCH_BENCH_ARGS="--ticks 6000 --threads 1,2,6,12" make match-bench
MATCH_BENCH_ARGS="--scale 1,5,10 --callgrind --warmup 100 --counted 3" make match-bench
```

Reports go to `.cache/match-bench/` (ignored). Wall time is on a shared machine with the load
recorded, not pinned cores.

Timed runs attach an index observer. `phases.index_collision` measures body relocation and
`phases.index_chunks` measures all-unit presence-bucket relocation across the tick, excluding
spatial queries. These spans are included in the outer tick phases; do not add them to movement
as separate work. Clocks accumulate nanoseconds before reporting milliseconds. Observer
overhead is part of the tick, so use the same observer on both versions in a comparison.

## Rung 0 (2026-11-09, revision 882e146d8)

100 seats, seed 1, 1,000 ticks, 6 workers, `--suite phase0`; the machine was shared (1-minute
load 38 to 67), so wall times move by up to 2x. Phase values are p50 ms over the run; the
whole tick includes the bots' route-heavy start (p95, p99 much higher). Waiting is
`route_scheduler_stats().waiting_units` at tick 1,000: this is a **starved baseline** (most
waiting units hold no planner: they wait for a search slot, and 92 to 97 players sit at the
per-player search cap).

| scale | units | buildings | map side | ticks/s | tick p50 | p95 | movement | gathering | building memory | waiting | RSS MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1x | 9,729 | 4,723 | 2,000 | 8.84 | 86 | 238 | 59.1 | 11.5 | 3.2 | 1,279 | 61 |
| 5x | 46,787 | 22,387 | 4,472 | 2.01 | 372 | 1,317 | 218.8 | 87.6 | 12.2 | 4,362 | 207 |
| 10x | 93,075 | 44,477 | 6,324 | 1.14 | 634 | 1,952 | 393.9 | 131.1 | 25.1 | 5,965 | 400 |

What breaks first as units grow (whole-population passes per tick, `pass_counts`):

- `collision_closed` visits grow far faster than units: 19.7k, 451k, 1.79M per tick (91x for
  9.6x the units). It is the first superlinear pass.
- Gathering grows faster than units (11.5, 87.6, 131 ms p50; p95 910 ms at 5x).
- Movement grows about linearly with movers (59, 219, 394 ms for 1.4k, 4.8k, 6.5k movers).
- `remember_visible_buildings` and `advance_gates` visit every building every tick (4.7k to 44k),
  `tables_refresh` 32k to 275k entries per tick: linear, not yet dominant.
- The route queue saturates: 1.3k, 4.4k, 6.0k units waiting, oldest wait the whole run at 5x
  and 10x.

1x is close to the owner's minute 33 (9,250 units, 97 ms tick p50, movement 54 ms).

## Worker determinism (2026-11-09, revision 882e146d8)

100 seats, 1x, seed 1, 3,000 ticks (2.5 game-minutes), hash every 100 ticks: the 31 canonical
hashes are identical at 1, 2, 6 and 12 workers (final `35589148f6dca3b9...`, 9,807 units, 255
army waves and 3 General marches issued by the bots). No divergence was found, so none was
fixed. The plan's gate asks for 30 game-minutes (36,000 ticks); that length was not run.

Workers barely help this load: 9.05, 10.22, 8.93 and 11.17 ticks/s at 1, 2, 6 and 12
(machine load 14 to 25), so most of the tick is serial today.

## 1,000 seats (2026-11-09, revision 882e146d8)

1,000 seats, 1x, 2,000 ticks, 6 workers, map side 6,324 (load 16 to 19). The world takes
`WorldLimits::for_match(1000)`; nothing else was raised. Setup 502 s: 83,205 units, 38,924
buildings, 956 garrisoned Stewards, no village refused.

| ticks/s | with bots | tick p50 | p95 | p99 | movement | gathering | wildlife | building memory | movers | waiting | RSS MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 0.69 | 0.55 | 1,181 | 3,042 | 5,786 | 787.8 | 282.1 | 31.1 | 21.0 | 19,373 | 18,083 | 706 |

At tick 2,000 the route scheduler's world-wide bound is reached (`world_capacity_exhausted`), 115
players sit at the search cap and 17,683 of 18,083 waiting units hold no planner: a starved
baseline. `collision_closed` visits 742k bodies a tick. The bots cost 353 ms a tick on average
(outside the tick timing, inside "with bots"): their every-second scan of all units is itself a
whole-population pass.

## Instructions per stage (Callgrind, 2026-11-09)

`--callgrind --scale 1,5 --warmup 100 --counted 3`, one worker, ticks 101 to 103, millions of
instructions per tick (simulated LL 8 MiB, host-derived). 10x was not run (setup alone would
take about an hour under Valgrind).

| scale | units | movers | tick | movement | group_prepare | formations | route_serve | propose | apply | sidestep | combat | wildlife | bookkeeping | DRAM MiB/tick |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1x | 9,692 | 1,636 | 1,312 | 1,199 | 737 | 777 | 403 | 13.4 | 4.8 | 4.1 | 3.4 | 9.2 | 0.9 | 15.6 |
| 5x | 46,763 | 5,758 | 2,906 | 2,692 | 1,691 | 1,979 | 544 | 139 | 28.8 | 22.7 | 1.5 | 14.4 | 2.2 | 49.2 |

`prepare_group_targets` (formation targets) is 56% to 58% of the counted tick, then route
search. `bookkeeping` here is only the index relocations, vision and `collect_changed`: the
gathering and building-memory phase functions are inlined in this build and have no symbol
(`present: false`), so they are **not measured** (their wall time is in the rung-0 table).
Three ticks right after warmup are not a steady-state sample; counts were not repeated, so the
plan's ±3% over 3 runs is not shown.
