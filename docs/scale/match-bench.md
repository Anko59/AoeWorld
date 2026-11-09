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
