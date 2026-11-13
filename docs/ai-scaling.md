# Long-run AI scaling benchmark

The scaling workload runs the ordinary Python Brain through the public SDK on
an unmodified source-backed France or Mekong package. Measurement uses the
admin API for authoritative kills and the bot's own economy and buildings for
its development. Admin observations are never passed into a Brain.

Required samples are 30, 60 and 120 game minutes, including eliminated slots.
A missing checkpoint/timing record, bot error, lost connection or timeout makes the
run incomplete; a partial game is never a completed baseline. The seed, package
hash, start anchors, actual starts, configuration and source revision identify
the workload. Network delivery remains a source of variation in the lockstep
arena, so comparisons require the same inputs and should report that limit.

For every slot and checkpoint record population, completed Town Centers, age,
researched technologies, banked food/wood/gold/stone, military units and kills.
`settled_cells` is a development footprint: the number of distinct 64-by-64 tile
cells containing a completed Town Center, Castle or military production
building. It measures spatial expansion, not exclusive ownership of terrain.
Record the building positions as well so the footprint is auditable.

Hard starts use explicit local anchors in `POST /match`. These retain the
normal village kit and bounded local village search, but omit the main-continent
and distant fallback searches. The response records actual scout positions and
the coarse hydrology main-land classification. A forest start must additionally
have at least 200 natural trees within 32 tiles of its starting village; the twelve kit trees
alone do not establish a forest start. A package name alone proves neither a
forest nor an island start.

Record tick cost distributions over the full run, per-bot CPU decision cost,
order totals and maximum orders per decision. Lockstep wall speed is diagnostic;
it does not establish the live server's tick rate or qualify dedicated hardware.
A p95 tick over 50 ms is outside the 20 Hz tick budget on the measured host.

Keep full JSON measurements, map packages, geodata and recordings outside Git.
Only the reproducible workload definition and measured summary belong here.

## Reproduce

The checked-in [France workload](../qa/ai-scaling.json) fixes four starts:

| slot | case | anchor | actual scout start | natural trees within 32 tiles |
| --- | --- | --- | --- | --- |
| 0 | island (off the main land mass) | 66, 51 | 63, 48 | 241 |
| 1 | forest | 1200, 600 | 1200, 600 | 576 |
| 2 | inland forest | 1000, 1200 | 1000, 1200 | 752 |
| 3 | open land | 700, 850 | 700, 850 | 0 |

Use the ignored map packages/geodata from the owner's play13 checkout, then run:

```sh
make ai-benchmark AI_BENCHMARK_ARGS='--map-hash dc7e0d4b253af4e7e7106b69eba61169ef2f8f6b0fd7781fbe5a3ed6ccc60e68 --starts qa/ai-scaling.json --out .cache/ai-scaling/baseline.json'
```

`--check-starts` validates the start cases without advancing the game. Output
paths must be fresh. The result JSON retains incomplete status until all three
checkpoints arrive. The adjacent `.ticks.jsonl` contains the full timing stream.
The server and SDK wheel use release builds; the benchmark runs one ordinary
Brain per slot, at the default two decisions per game second.

During a live run, `.cache/ai-scaling/live.json` identifies its local server.
`/pkg/ai-view.html` opens slot 1 through the ordinary browser slot token for
filming with `make showcase`. This ignored temporary bootstrap contains a local
slot credential and is removed on exit. It sends no orders and keeps fog on.
Keep the recording outside Git and attach it with `SHIP_VIDEO` when shipping.

## Recorded baseline

Measured 2026-11-13 from AI revision `a87da7b00b7ac2ae5edd62e13beaae0edd19c6e6`,
with the measurement instrumentation introduced here. The unchanged Brain
and configuration are the comparison point for subsequent behavior changes.
Full evidence is `.cache/ai-scaling/baseline-recorded.json` and its adjacent
`.ticks.jsonl`; source SHA-256 values are embedded in the JSON.

| minute | slot | pop | TCs | settled cells | age | techs | food / wood / gold / stone | military | kills |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 30 | 0 | 99 | 1 | 2 | Castle | 5 | 2 / 20 / 46 / 200 | 61 | 0 |
| 30 | 1 | 67 | 1 | 1 | Castle | 5 | 24 / 75 / 210 / 200 | 36 | 0 |
| 30 | 2 | 47 | 1 | 2 | Feudal | 3 | 1850 / 3771 / 115 / 200 | 16 | 0 |
| 30 | 3 | 38 | 1 | 2 | Feudal | 3 | 2920 / 12 / 395 / 200 | 7 | 0 |
| 60 | 0 | 295 | 1 | 2 | Castle | 8 | 435 / 78 / 37 / 200 | 233 | 0 |
| 60 | 1 | 193 | 1 | 2 | Castle | 5 | 19 / 55 / 40 / 200 | 161 | 0 |
| 60 | 2 | 46 | 1 | 2 | Feudal | 3 | 5875 / 15698 / 115 / 200 | 16 | 0 |
| 60 | 3 | 117 | 1 | 2 | Castle | 5 | 1175 / 7 / 3729 / 200 | 55 | 1 |
| 120 | 0 | 295 | 1 | 2 | Castle | 10 | 10243 / 16457 / 37 / 200 | 233 | 0 |
| 120 | 1 | 365 | 1 | 2 | Castle | 10 | 6333 / 10346 / 43 / 200 | 316 | 0 |
| 120 | 2 | 34 | 1 | 2 | Feudal | 3 | 15980 / 38178 / 115 / 200 | 4 | 20 |
| 120 | 3 | 289 | 1 | 2 | Imperial | 8 | 13 / 2435 / 899 / 200 | 229 | 13 |

| minute | median pop | median TCs | median age | median techs |
| --- | --- | --- | --- | --- |
| 30 | 57 | 1 | 1.5 | 4 |
| 60 | 155 | 1 | 2 | 5 |
| 120 | 292 | 1 | 2 | 9 |

Age medians use Dark=0, Feudal=1, Castle=2, Imperial=3.

Run diagnostics:

```json
{
  "ticks": {
    "count": 144009,
    "tick_p50_us": 5992,
    "tick_p95_us": 16036,
    "tick_max_us": 1413698,
    "orders": 24108
  },
  "wall_seconds": 3006.180930429022
}
```

| slot | CPU ms/decision | slowest CPU ms | slowest wall ms | max orders/decision | orders |
| --- | --- | --- | --- | --- | --- |
| 0 | 40.019 | 688.175 | 841.302 | 14 | 5193 |
| 1 | 32.476 | 337.255 | 337.721 | 32 | 10116 |
| 2 | 9.098 | 189.154 | 278.324 | 7 | 2399 |
| 3 | 27.360 | 200.237 | 200.247 | 18 | 6405 |

This is one four-player run on a shared host, with actual game footage recorded
during it. It establishes a reproducible behavior baseline; it does not
establish 100-player performance or dedicated hardware qualification.
The earlier `.cache/ai-scaling/baseline.json` attempt was stopped after
detecting absent arena tick records; it is incomplete and is excluded.

## Kingdom development

The default workforce target grows with completed Town Centers. The next Town
Center target grows with surviving villagers, so losing a center or workforce
reopens production and construction. Expansion searches around living centers
and checks separation from every existing center. Gathering, farms, houses and
production use the surrounding settlements instead of a permanent first-base
radius. Castle and production counts grow with the settlement count.

Stewards, Vassals and Generals are enabled by default. Ordinary SDK controls
activate Steward training, pause management spending while research or another
manager is being funded, and enable Vassal fortification once Castle reserves
are satisfied. A small local workforce is reserved for Vassal construction.
Research plans read the complete current technology tree from Rust, including
age and technology prerequisites, and reserve late-game upgrade prices ahead
of continuous military production. Markets trade one lot per eight game
seconds and use observed prices including the server's commission. Sale
proceeds are available only after the next authoritative observation.

The host issues at most 40 orders per decision (80 per game second at the
default two decisions per second). Each manager gets a bounded share and build
site queries have a shared four-query budget. Town Center and military queues
rotate through the owned production buildings. Manager and fortification counts complement
the host command counts.

The SDK exposes `technologies()`, `market_quote(price, buy)`,
`Client.steward_settings(unit, weights, cap=100, training=True)` and
`Client.vassal_development(unit, enabled)`. The Python layer converts types;
canonical prices and technology requirements stay in Rust. Run the focused
binding regressions with:

```sh
make sdk-python-test SDK_PYTHON_TEST_ARGS=--bindings-only
make sdk-python-logic-test
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-sdk AOE_NATIVE_TEST_FILTER='test(technology_catalog_covers) | test(management_settings_use) | test(market_quotes_include) | test(rules::) | test(orders::) | test(steward::)'
```

The benchmark additionally reports completed Castles, towers, wall/gate segments
and management-unit counts. Forest clearing, island transports, bridges and
forward force projection remain separate follow-up increments.

The first scaling prototype was stopped after its 60-minute checkpoint:
median population 161.5, Town Centers 2, age 2.5. Its modest population gain and
absence of Stewards/Generals exposed research and management spending
starvation. `.cache/ai-scaling/scaling.json` remains incomplete and is excluded
from the final comparison. Queue drains now require the padded research price;
science preserves the first managers' prices, and Imperial prerequisites are
funded before more farms and drop-offs.

The subsequent priority prototype reached median population 156.5, Town Centers
2 and age 2.5 at 60 minutes, with management units active. It was also stopped
and remains incomplete in `.cache/ai-scaling/scaling-priority.json`. The next
workload uses a 30-villager expansion trigger, preserves a 100-wood float for
essential work, and pauses further army spending while an established army
protects an unpaid expansion. Defeated workforces can still re-boom.

## Budget-limited kingdom measurement

The four-hour session budget ended before the final 120-minute checkpoint.
The growth tree remains unqualified for the full requested win condition.
Its source/configuration and observed prefix remain in
`.cache/ai-scaling/scaling-growth.json` and the adjacent `.ticks.jsonl`.

| minute | version | median pop | median TCs | median age | median techs | median settled cells |
| --- | --- | --- | --- | --- | --- | --- |
| 30 | baseline | 57 | 1 | 1.5 | 4 | 2 |
| 30 | kingdom | 53 | 1 | 2 | 4 | 1.5 |
| 60 | baseline | 155 | 1 | 2 | 5 | 2 |
| 60 | kingdom | 173 | 3 | 3 | 5 | 3 |
| 120 | baseline | 292 | 1 | 2 | 9 | 2 |
| 120 | kingdom | not measured | not measured | not measured | not measured | not measured |

Age rank: Dark=0, Feudal=1, Castle=2, Imperial=3. The observed 60-minute
population gain is 12%, with three times as many Town Centers and a full
age-tier increase. No towers or walls were observed at that checkpoint.
A separate observation at 92.43 minutes confirmed two completed watchtowers,
86 completed stone-wall segments and two completed stone gates for slot 1.
Near 89.57 minutes, an extra live observation had median population 315,
median Town Centers 4 and all four players in Imperial Age. These extra
observations do not replace the missing 120-minute checkpoint.

| minute | slot | pop | TCs | cells | age | techs | food / wood / gold / stone | military | kills |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 30 | 0 | 73 | 2 | 3 | Castle | 4 | 338 / 80 / 505 / 170 | 33 | 0 |
| 30 | 1 | 53 | 1 | 1 | Castle | 5 | 163 / 51 / 257 / 190 | 22 | 0 |
| 30 | 2 | 53 | 1 | 1 | Castle | 4 | 213 / 225 / 334 / 180 | 21 | 0 |
| 30 | 3 | 41 | 1 | 2 | Feudal | 2 | 2888 / 3 / 585 / 790 | 10 | 0 |
| 60 | 0 | 213 | 3 | 4 | Imperial | 5 | 466 / 18 / 657 / 440 | 135 | 0 |
| 60 | 1 | 179 | 3 | 3 | Imperial | 6 | 733 / 841 / 1342 / 170 | 115 | 0 |
| 60 | 2 | 167 | 2 | 2 | Imperial | 5 | 1128 / 92 / 910 / 1287 | 110 | 3 |
| 60 | 3 | 94 | 3 | 3 | Castle | 4 | 1185 / 103 / 768 / 33 | 27 | 2 |

Validation passed: 188 AI decision tests, 23 binding/local-server tests,
13 focused Rust SDK tests, 4 harness tests and all required static gates.
The actual 59.88-second after video is `.cache/showcase/ai-kingdom.webm`.
Full suites, CI, 100-player scale and dedicated hardware remain unqualified.
