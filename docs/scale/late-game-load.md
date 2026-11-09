# Late game: the owner's 100-player load

Part of [the real game at scale](match.md).


`--late-game` seeds the shape of the owner's late game of 2026-11-06 (34
minutes, 100 seats, 9.5-10.1 ticks/s, evidence in the play checkout's
`.cache/game-2026-11-06/`) instead of a Dark Age start, so the per-phase
profile of a real late game can be reproduced and optimised:

```sh
SCALE_MATCH_ARGS="--late-game --players 100 --hosts 8 --minutes 12 --port 8145 \
  --map package --map-hash dc7e0d4b253af4e7e7106b69eba61169ef2f8f6b0fd7781fbe5a3ed6ccc60e68 \
  --package-directory local-assets/map-packages --label late-game --no-summary \
  --out .cache/late/late.json" make scale-match
```

The owner's world at minute 33: 9,207 units (3,355 villagers, about 5,700
soldiers, 120 fishing ships, 104 galleys), 3,479 units moving, 664 attacking,
about 1,800 villagers gathering, farming or returning; 4,355 buildings (80
Town Centers, 16 Castles, 220 stone wall tiles, 12 bridges, 657 farms, 1,878
houses); AIs 69 Castle, 17 Feudal, 10 Imperial and 4 Dark. Slot 0 alone had
1,650 population, 25 Town Centers, all the Castles, walls and bridges and an
870-soldier army.

`start_units.late_game` (`crates/server/src/gameplay_match/late_game.rs`,
seeding in `GameWorld::seed_late_game`) gives slot 0 that base (Imperial, 728
villagers, 51 fishing ships, 856 Imperial soldiers, 24 extra Town Centers, 16
Castles, 381 farms, 259 houses and the other buildings, 220 wall tiles on a
ring with gaps, up to 12 finished bridges over river water within 200 tiles)
and every AI seat a Castle Age start (every tenth Imperial, every sixth
Feudal) with 27 villagers, a fishing ship, 48 soldiers, a galley, farms,
mills, camps, houses and military buildings. Slot 0 holds 20,000 of each
resource and an AI seat 1,500 (with 20,000 the bots trained to 14,000 units in
12 minutes, past the owner's 9,200); the villagers start through the ordinary
farm and gather orders. The land around each village is worked out first, as
in the owner's two-hour world: every tree, mine and bush within 90 tiles of
slot 0's village and 30 of an AI's is exhausted (fish stay), and the seeded
non-farmers take the nearest nodes beyond that disc. Placement
walks fixed tile orders, so the same map, seed and seat give the same world.
The bots play their seats from there (their armies march when they find a
target). Slot 0 is played by a scripted human (`late` in the report): every
45 s it splits its army into four groups and sends each a Line attack-move
300 to 700 tiles away, the direction turning each round.

The report adds `phases_per_minute`: the server's rolling p50/p95 per phase
(`GET /admin/perf`, ms), the same numbers the owner's sampler recorded, and
its `routes`: the route queue then (`GET /admin/match/routes`). That answer's
`queue` splits every unit that waits for a route or still holds a planner by
requester side (human or AI seat), by what asked (`gather_trip`,
`single_move`, `group_carrier`, `member_detour`, `ship`, `attack_approach`)
and by reach (near up to 64 tiles, far, or `unplanned`: no planner yet, so the
destination is not known), with wait percentiles in ticks per kind and the
five players with most waiting units; the cumulative ends (`expired`,
`single_expired`, `group_orders_expired`, `group_members_expired`,
`detours_declined`, `detours_at_capacity`, `slot_refusals`) give expiries and
declines between samples.

### Late-game runs (2026-11-07, revisions 947a3ccc and its queue commit)

100 seats, package dc7e0d4b (France, 2,000 tiles), seed 1, 8 bot hosts, on the
shared development machine (1-minute load 9 to 43 from other agents' builds,
tests and probes), so absolute numbers move by up to 2x between runs. Phase
values are the server's rolling p50 (ms), medianed over the run's minutes.

| | owner (minute 33) | run 1 (AI stock 20,000) | run 3 | run 4 |
|---|---|---|---|---|
| units | 9,250 | 9,830 to 14,170 | 9,830 to 13,190 | 10,000 to 12,800 |
| ticks/s | 10.1 | 10.4 | 13.2 | 16.2 |
| tick p50 | 97 | 83 | 66 | 57 |
| movement | 53.9 | 45 | 37 | 31 |
| search | 9.4 | 9 | 8.5 | 7.6 |
| planning | 56.5 | 9 | 8.5 | 7.6 |
| plan_fast | 48.3 | 3 | 3 | 3 |
| gathering | 21.3 | 5 | 4.8 | 4.0 |
| apply_loop / group_apply | 4.5 / 5.3 | 11 / 10 | 9.2 / 10.0 | 7.9 / 7.5 |
| sidestep | 2.1 | 6.1 (one sample) | 6.5 | 4.3 |

Movement, search and the whole tick are within 1.5x of the owner's; planning
(plan_fast, plan_serve) and gathering are not: here `search` is all of
`planning`, while the owner's plan_fast spent about 39 ms outside `search`.
The owner played an older build (play/all-2026-11-06 on dev 4953b6aec); dev
has since changed route work (#716, #731, #732: ship searches 3.45x), so the
planning gap may be code rather than load (not measured: the old build was not
run on this load). Gathering stays low: the owner's 2-hour world had depleted
nearby resources (`resources` 2.2 ms, `terrain_point` 2.8 against 0.7 and 0
here); the seeding does not deplete.

Route queue (`routes` per minute): run 1 at minute 7, 328 waiting, oldest
2,388 ticks, 45 players at the search cap; runs 2 to 4 (AI stock 1,500) 22 to
304 waiting, oldest 600 to 1,200 ticks, 10 to 43 players at the cap, rising
with the minutes. The owner's: 655 to 688 waiting, oldest about 3,300, 40 to
44 at the cap, player 0 most waiting (163). At the end of run 3 (10 min):
303 of 304 waiting units were AI seats', 235 without a planner (waiting for
a search slot, destination not yet known); by kind single moves 139 (wait
p50 368 ticks, p99 1,119), gather trips 104 (p50 210, p99 1,093), group
carriers 46 (p50 463), attack approaches 15, member detours and ships 0. Over
the 10 minutes 221 waits expired (219 single units), 5,035 member detours
were declined (4,973 at capacity) and 31,508 slot refusals. Slot 0's scripted
army (four formation orders every 45 s) almost never waits, unlike the
owner's player 0: its orders are few and grouped, so this load does not
reproduce the owner's own queue share.

Run 3 carried an experiment (the sidestep candidates' building and terrain
reads ahead of the body sweep, a pure reordering) and run 4 is the same code
without it: sidestep 6.5 against 4.3 ms, movement 37 against 31, on a machine
whose load fell between the runs. No gain shows, so it was not kept.

### Profiling a run

`--profile <dir>` (with `--profile-after` seconds from server start, default
300, and `--profile-seconds`, default 60) builds the server with its opt-in
sampling profiler (`--features sampling-profiler`: `pprof`, line tables for
inlined frames; normal builds do not compile it) and writes `flame.svg`,
`stacks.folded` and `top.txt` (threads, self and inclusive frames) into the
directory; keep it under `.cache/`. Samples are 499 Hz of process CPU time
over every thread, so the tick thread's share is the stacks under
`advance_world`; the rayon workers (movement proposals, session builds) and
the async fan-out are beside it.

The depleted late game (2026-11-07, dev 19dc7fe4 plus the depletion, 90 s
from about minute 3.5) spends 58% of the process's samples on the tick thread:
movement 70% of it (formations 20%, route serving 17%, body sweeps
`collision_sweep_bounded` 19%, `bridge_connections` 9% across route search
and deck checks), gathering 9.5%, combat 9.4% (attack-position search 4.5%),
building memory 5.6%. Depletion did not raise the gathering phase (3.8 to
3.1 ms against the owner's 21): far walks cost movement and planning.

Optimisations from that profile, one 8-minute run each (p50 medians of
minutes 3 to 8, ms; shared machine, so a run moves by several percent):

| | dev | + depletion | + building memory, single file | + bridge filter | + deck reject, player index |
|---|---|---|---|---|---|
| ticks/s | 17.0 | 18.4 | 18.2 | 19.4 | 19.4 |
| tick | 56.4 | 51.9 | 52.5 | 48.9 | 48.0 |
| advance | 42.6 | 39.8 | 40.2 | 37.4 | 36.1 |
| movement | 30.1 | 29.5 | 29.7 | 27.2 | 26.8 |
| search | 7.2 | 8.3 | 9.3 | 8.0 | 7.9 |
| gathering | 3.8 | 3.1 | 2.9 | 2.9 | 2.8 |

In profile shares of the tick thread: building memory 5.6% to 3.7%,
`single_file_group` 1.8% to 0.5%, `bridge_connections` 8.8% to 2.4%,
`deck_offset` 3.3% to 1.0%. Body sweeps (18%), formations and route
serving stay the largest costs.

Round 2 (same load): `ResourceOverlay::remaining` answers a recorded amount
without decoding its page (0.99% to 0.47% of advance), the attack-position
search keeps its costs in a hash map (4.2% to 3.3%), delivery patches
resident units with one forward walk; none is visible in the phase p50s
(tick 47.8 to 50.3 ms between runs on the shared machine).

Why the owner's gathering phase was 21 ms and ours 3: the timer has the
same boundaries in the owner's build 4953b6aec and now (the probe after
construction to the probe after `advance_gathering`, resource sink
included), and the gathering code differs only by an unused save helper.
The load differs: at minute 4 here 397 villagers gather and 220 farm (of
4,231; 1,499 walk, 756 shepherd); the owner's world had 765 and 544 (of
3,355) and its resource-block cache was full (8,192 entries, LRU) all match
long. Shrinking that cache to 512 entries changed nothing here, because
our gatherers barely search for replacements (block builds 0.03%): what
makes the owner's 21 ms is not reproduced, and profiling a resumed save of
the owner's match (`#718`) is the direct way to find it.

