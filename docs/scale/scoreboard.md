# Scale scoreboard (issue #865, Q1)

One frozen workload judges every scale change: the owner's Anatolia save
`1794783742-anatolia` (tick 185,423, 45,447 units, 34,575 buildings, 99 AI
seats, map `61633e71…`), from the 11-15 owner game. Saves, packages, command
streams and reports stay under `.cache/`, never Git. A later save of the same
game at about 62k units (`1794866709-anatolia-2`) is kept for higher rungs.

## Inputs

- Copy the save and its card from `/home/anko/Work/evidence/game-2026-11-15/`
  into `.cache/scoreboard/packages/saves/` (real copies; never edit the
  originals). Hard links are fine for the package's `pages/` and
  `navigation/`.
- Asset pack `8f7abe8c…` and its sibling packs, hard-linked under
  `local-assets/packs/`.
- Command stream `.cache/scoreboard/commands-750.jsonl`: 750 ticks
  (185,423–186,172), 12,103 commands, recorded live on `5c34c215a` with
  `--record-commands`. SHA-256 of the save:
  `63b45874ed0cd4f39a2fed5337599bd273233d9e1ae1621b3ded64cad8796dc2`.

```sh
make scale-match SCALE_MATCH_ARGS='--save .cache/scoreboard/packages/saves/1794783742-anatolia.save --package-directory .cache/scoreboard/packages --asset-pack local-assets/packs/8f7abe8c330d7fd710daa076c8ef07de65f8973feed59b99f4d01f9cbc9c4530 --minutes 5 --port 9388 --label q1 --no-summary --out .cache/scoreboard/runs/live.json'
make save-replay AOE_SAVE_REPLAY_ARGS='--save .cache/scoreboard/packages/saves/1794783742-anatolia.save --package-directory .cache/scoreboard/packages --recording .cache/scoreboard/commands-750.jsonl --runs 1 --workers 1 --out .cache/scoreboard/runs/replay-w1.json'
```

Run the replay a second time with `--workers 6` for main-thread instructions.
A behaviour change shifts entity ids and invalidates the stream; record a new
one on the base revision before comparing, and say so.

## Baseline, 2026-11-17, revision `5c34c215a`

`dev` `b27321d21` plus the owner build's content (#872) and the replay work
numbers of this change.

| Measure | Value |
| --- | ---: |
| Live ticks/s, 99 AIs, 285 s (load 3.3 at start) | 2.63 |
| Live tick p50 / p95 | 368 / 511 ms |
| Live advance p50 | 250 ms |
| Replay instructions/tick, 1 worker (p50, p95) | 1,106.1M (1,061M, 1,524M) |
| Instructions/tick per restored unit | 24.3k |
| Main-thread instructions/tick, 6 workers | 839.2M |

Replay phases, 1 worker, millions of instructions per tick (they nest):
movement 758.5, group_apply 242.6, sidestep 183.0, combat 181.8, plan_serve
133.5, group_propose 118.2, apply_loop 114.4, search 83.5, plan_fast 75.4,
steward_economy 61.2, group_prepare 51.5, propose 42.0, gathering 35.1.
Live phases, p50 ms: movement 157, combat 54, group_apply 52, apply_loop 38,
plan_serve 22, index_collision 18, fanout_wait 16.

Work in the replay window: 12,103 commands, 2,340 refused; 23,136 hits, 363
kills; 17.8k–19.4k units with a movement goal; units waiting for a route
6,444, 5,363, 4,711, 3,638, 3,715, 3,763, 3,760 at each 100 ticks;
`slot_refusals` 329,609; 29–33 players at their search cap; 121,772
resources delivered by the end of the window. Gather trips, single moves and
group carriers that wait at the end of the window have waited all of it.

The report's `queue_series` carries these every 100 ticks, with `movers` and
`delivered_last_minute` (income is not saved, so it starts empty at the
restore).
