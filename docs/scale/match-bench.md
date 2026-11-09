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
  (runs in separate processes).
- **Instructions.** `--callgrind` re-runs the binary under Callgrind (cache simulation, one
  worker): `--warmup` ticks uninstrumented, then `--counted` ticks collected only inside
  `counted_tick`. The profile is split into the stages of `perf-stage-counters` plus whole tick
  phases and a `bookkeeping` sum (index relocation, vision, collect_changed, building memory).
  Simulated LL misses x 64 B stand in for DRAM bytes: `perf_event_paranoid` is 4 here.

```sh
MATCH_BENCH_ARGS="--ticks 2000 --threads 6 --scale 1,5,10" make match-bench
MATCH_BENCH_ARGS="--seats 1000 --ticks 2000" make match-bench
MATCH_BENCH_ARGS="--ticks 6000 --threads 1,2,6,12" make match-bench
MATCH_BENCH_ARGS="--scale 1,5,10 --callgrind --warmup 100 --counted 3" make match-bench
```

Reports go to `.cache/match-bench/` (ignored). Wall time is on a shared machine with the load
recorded, not pinned cores.

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
