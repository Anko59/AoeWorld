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
before run. A replay test (`gathering_and_dropoffs_replay_to_pinned_hashes`) pins
six hashes taken on 41d85f46.

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
