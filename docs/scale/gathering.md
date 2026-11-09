# Gathering and collation measurements

Hot spots 7 and 8, measured with `make perf-scale` (100 players, 101,111 units,
fog on, 600 ticks) on revision 42d76b22 against the same run on 41d85f46
(`reports/scale/before-gather.json`, `after-gather.json`; local, not committed).
Other agents loaded the machine during the after run (movement, untouched, read
356 then 368 ms p50), so read the gains as approximate.

| Phase (ms per tick) | before p50 / p95 | after p50 / p95 |
|---|---|---|
| gathering and economy | 20.7 / 25.8 | 8.3 / 11.6 |
| result collation | 10.6 / 12.6 | 7.5 / 9.4 |
| whole `advance` | 469 / 581 | 471 / 567 |
| initial node search (`find_nodes_s`, s) | 5.6 | 9.7 |

Canonical hashes at ticks 100 to 600 and the initial hash are identical to the
before run. A replay test (`gathering_and_dropoffs_replay_deterministically`)
runs the scenario twice and requires the six checkpoint hashes to agree.

What changed: a per-player, per-kind drop-off index (`Economy::dropoffs`); the
replacement search reads cached 8x8-tile blocks ring by ring with the former
`(distance, id)` tie-break; phases push entity ids, and the tick sorts, dedups
and fetches final states once; walking gatherers skip the task-map round trip;
a harvest no longer clones the resource overlay.

Limits: the targets (gathering at most 4 ms, collation at most 1 ms, initial
assignment at most 0.5 s) are not met. Initial assignment got slower: it is
bound by first-touch terrain evaluation (`object_at`), which a block cache cannot
avoid, and the block lists add evaluation of tiles the old circle skipped.
Collation is still a sort plus 43,000 `BTreeMap` fetches; a dense dirty list is
not done. The whole tick is unchanged because movement (about 300 ms) dominates.

## Economy sleep (movement rework phase 7b, gathering)

A worker at its resource only adds a constant credit each tick until its next
harvest, so it sleeps until that harvest tick (`economy/gathering/sleep.rs`).
Skipped credits are reconstructed exactly: the canonical hash reads
`credits_at(tick)`, never the stale stored value. A drop-off waiter skips until
its retry tick.

Sleepers doze (`economy/gathering/dozing.rs`): the loop walks only awake
jobs, in id order, and a job begun during the loop waits for the next tick as
before. A dozer returns at its harvest tick or on a wake event from every
writer of what the pure step reads: its job (insert, remove), its unit (ids
changed this tick before gathering, ids changed since last tick's gathering,
`remove_unit`), its player's technologies (`complete_research`) and its target
(a node or carcass emptied, a farm removed or reseeded, a target crossing the
crowd mark). A woken dozer still runs the full check, so a spurious wake is
exact. In test builds every dozer is re-checked before each loop and a stale
one panics (the missed-wake detector); the whole `aoe-simulation` suite runs
with it. Tests that edit state directly post the wake by hand
(`wake_gatherer`, `wake_player_gatherers`, `wake_target`).

The oracle switch is `GameWorld::set_work_sleep(false)` (`perf-late-game
--no-work-sleep`, `match-bench --no-work-sleep`). Replay oracles compare sleep on
and off: `benchmark::tests::sleep` (8 seats for 2,400 ticks, hash every tick;
the 14,000-unit late-game workload for 512 ticks), `gathering::replay` (a node
exhausted, gold mining and wheelbarrow landing under sleepers, a raid on a
miner, hash every tick) and `farming_sleep_on_and_off_agree_through_a_reseed`
(9,000 ticks).

Measured (test profile: opt 2 with debug assertions, shared machine, on and
off interleaved in one process, ticks 385 to 512 of the 14,000-unit workload):
gathering p50 7.38 to 5.28 ms, p95 10.94 to 8.42 ms; whole tick p50 51.3 to
48.3 ms. At tick 400, 3,583 gatherers sleep, 3,841 approach and 574 return
under orders: walkers, not workers, are now most of the phase.

Hunters on a carcass doze too (a carcass does not move). Farmers waiting for
FARM production still step every tick; on the match-bench load they waited 0.1
steps per tick at 1x and 0.0 at 5x, so a lazy wait was not built. Builders,
trainers and attackers are unchanged.

## Why gathering grew faster than units (match-bench)

`GameWorld::gather_work` counts the phase's work (also in the match-bench
report as `gather_work`). In-process, 16 seats, bots, ticks 301 to 600, test
build without the detector, shared machine:

| | 1x (2,820 units) | 5x (14,029 units) |
|---|---|---|
| drop-off choices per tick | 0.7 | 2.4 |
| buildings ranked per choice, before | 48 | 166 |
| perimeter tiles ranked per tick, before / after | 157 / 23 | 4,621 / 223 |
| route attempts per choice (all but 1-2% unreachable) | 25 | 86 |
| gathering mean, before / after lazy ranking | 1.61 / 1.46 ms | 14.81 / 6.33 ms |
| gathering p50, sleep off / on (true sleep) | 412 / 91 us | 2.42 / 0.86 ms |

Every drop-off choice ranked the perimeter of every drop-off building its
player owned, with a body sweep per tile, before trying the first: 5x the
buildings per seat times 5x the returns. Ranking is now lazy, nearest building
first, in the same attempt order (the pinned late-game checkpoint is
unchanged). Still superlinear: the unreachable route attempts per choice, from
node workers (farmers make none), refused by the route scheduler; and the mean
is dominated by spikes (p95 20 to 40 ms at 5x) from returns and replacement
searches.
