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
make ai-benchmark AI_BENCHMARK_ARGS='--map-hash fdc59853e2404cb5c7fb245f21768970aeb01febe20fd4f15dc96ff522fc9a49 --starts qa/ai-scaling.json --wall-minutes 100 --out .cache/ai-scaling/baseline.json'
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

The historical measurements below used recipe 3 package
`dc7e0d4b253af4e7e7106b69eba61169ef2f8f6b0fd7781fbe5a3ed6ccc60e68`.
The reproduction workload now names regenerated recipe 4 France, with the same
request, seed and anchors. Current code rejects recipe 3; compare AI changes on
the current package before attributing differences to the planner.

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
manager is being funded, and enable Vassal fortification using its native
resource floor when a local threat justifies development. A small local
workforce is reserved for active Vassal construction.
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

## Settlement fortifications

The Python planner builds Watch Towers at completed resource drop-offs and on
reachable approaches from remembered enemy buildings. The tower allowance grows
by one per thirty surviving villagers. The approach probe samples three local
cross-sections, prefers narrower explored ground, and never treats unknown
terrain as a natural wall. Castles prefer the enemy-facing edge of the nearest
frontier settlement; without a known enemy they retain settlement placement.

At sixty villagers, a known approach can receive a Stone Gate and two short
wall wings. The gate is paid first and must finish before the wings start.
The four-tile opening remains clear; east/west approaches use stepped wings
around the ordinary horizontal gate. Exact wall sites cannot drift into the
gate opening when a site search encounters an obstruction. Lost wall segments
can be rebuilt, and paid foundations/plans count toward coverage.

One construction order is allowed per twelve game seconds. Defences share the
Brain's forty-order decision ceiling and public route/site query budgets, and
leave fifty wood and a hundred stone for essential development. An expansion's
existing reserve is respected without reserving its wood float twice. Builders
can use any of the four footprint edges when forest blocks one approach. Native
Vassal development remains active alongside this planner. Towers and walls do
not wait for an expensive Vassal to train and garrison.

Seated Vassals develop defences at threatened Castles, even while future Castles
are being funded. Their native planner already preserves 200 food, wood and
stone and 100 gold. Peaceful Castles neither recruit nor save for new Vassals;
they can recruit Generals directly. When local danger passes, development is
disabled and the host releases its reserved workers back to economic work.

The host requires an enemy unit or recent damage/loss within 96 tiles, or a
remembered enemy building within 192 tiles. Damage and own unit/building losses
authorize spending locally for three game minutes; distant army losses do not
fund home fortifications. Alert memory holds at most 64 eight-tile cells. A
decision-local spatial index avoids a full enemy scan for every building, with
no extra land/site queries. Towers require threatened resource sites and are
capped by both workforce and two towers per threatened settlement. Income alone
does not authorize construction. Existing foundations can still be completed.
Castle stone is allocated only once Castle Age unlocks the building. The old
Feudal allocation diverted fifteen percent of workers before the wood-starved
land start could fund its army and Castle Age prerequisite buildings.
Known, reachable timber can be harvested up to 256 tiles from a settlement;
other resources retain the 70-tile radius. A remote cutter can fund a nearby
Lumber Camp, including after starting timber is exhausted. The existing shared
route/site budgets and rotating drop-off inspection still apply. Fishing
continues normally, but defensive galleys require an enemy warship near the
single admitted Dock producer. Quiet water no longer buys a war fleet.

Unfunded optional Imperial research reserves only the portion of its price
already owned. Reserving nonexistent gold previously made even food-only
villagers fail the shared affordability check. Surplus food can now keep
villagers growing while army spending yields to the upgrade. Town Centers
drain for an optional upgrade only once its padded price is actually available;
missing gold no longer pauses every settlement's villager queue.

The current benchmark separates completed wall tiles from gates and retains
every completed fortification's position. Each row also names its start case.
Slot 2 is classified as forest, matching its already-measured 752 natural
trees; anchors, seed, package and actual village starts are unchanged.

## Fortification measurement

Measured 2026-11-13, same package, seed and village anchors as the baseline.
Age ranks are Dark=0, Feudal=1, Castle=2, Imperial=3.

| minute | source | median pop | median TCs | median age rank |
| --- | --- | --- | --- | --- |
| 30 | original baseline | 57 | 1 | 1.5 |
| 30 | starting kingdom | 53 | 1 | 2 |
| 30 | fortifications | 60.5 | 1.5 | 2 |
| 60 | original baseline | 155 | 1 | 2 |
| 60 | starting kingdom | 173 | 3 | 3 |
| 60 | fortifications | 184.5 | 2.5 | 3 |
| 120 | original baseline | 292 | 1 | 2 |
| 120 | starting kingdom | not measured | not measured | not measured |
| 120 | fortifications | 716 | 6.5 | 3 |

| minute | slot / case | pop | TCs | age | castles | towers | walls | gates |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 30 | 0 / island | 59 | 1 | Castle | 1 | 0 | 0 | 0 |
| 30 | 1 / forest | 62 | 2 | Castle | 1 | 0 | 0 | 0 |
| 30 | 2 / forest | 62 | 2 | Castle | 1 | 1 | 0 | 0 |
| 30 | 3 / land | 45 | 1 | Feudal | 0 | 0 | 0 | 0 |
| 60 | 0 / island | 160 | 2 | Imperial | 1 | 1 | 0 | 0 |
| 60 | 1 / forest | 221 | 3 | Imperial | 2 | 2 | 0 | 0 |
| 60 | 2 / forest | 209 | 3 | Imperial | 2 | 2 | 0 | 0 |
| 60 | 3 / land | 89 | 2 | Castle | 1 | 1 | 0 | 0 |
| 120 | 0 / island | 1053 | 7 | Imperial | 2 | 10 | 33 | 3 |
| 120 | 1 / forest | 657 | 6 | Imperial | 2 | 4 | 90 | 2 |
| 120 | 2 / forest | 775 | 9 | Imperial | 4 | 15 | 322 | 8 |
| 120 | 3 / land | 216 | 5 | Imperial | 1 | 3 | 62 | 6 |

Complete run: 4085.39 wall seconds; 144002 timing records. Tick p50/p95/max:
6491/44244/865739 microseconds; 60275 applied orders. Tick p95 is inside the 50-ms
diagnostic budget. No reported bot errors, drops or host decision timeouts. Hard-start
median TCs at 120 minutes: 7; ordinary slot's 60-minute TCs: 2.

| slot | CPU ms/decision | slowest CPU ms | slowest wall ms | max orders/decision | orders |
| --- | --- | --- | --- | --- | --- |
| 0 | 24.92 | 178.68 | 339.27 | 16 | 10026 |
| 1 | 38.59 | 732.70 | 789.03 | 17 | 16780 |
| 2 | 30.91 | 209.83 | 582.22 | 20 | 21456 |
| 3 | 15.99 | 185.67 | 202.18 | 16 | 12021 |

Evidence: `.cache/ai-scaling/fortifications-vassal.json` and adjacent `.ticks.jsonl`;
the 1029-file source SHA-256 manifest was taken from tree
`cef11537f56df6024eaa89d30320de692d9fcfd0` (cherry-picked onto dev as `426b3c4d5`), the
only measured revision. The benchmark was not rerun after later commits (including the
fortification lane, gate-fit and castle-reachability fixes), so those are not remeasured
and the manifest does not describe the current tree. The JSON revision names the starting
HEAD, not the later commit.
The three prototype runs are incomplete and excluded. The funded prototype was
deliberately interrupted at minute 96 after exposing the Vassal gate; its Make launcher
had ended with exit 143 while the original Docker child remained live.

One four-player source-backed France run on a shared host, with filming and client
preparation during the run; network delivery changes decisions. Lockstep timings do not
qualify live tick rate, instructions per unit, the 100/1000-player scale ladder or
dedicated hardware. This is not a loss/re-boom qualification. Forest clearing policy,
island transports and a mainland Town Center remain unfinished. Host known-enemy lane
geometry and frontier placement have fake-world coverage; this benchmark qualifies
counts, not defensive efficacy. Comparison against the original baseline includes the
earlier kingdom increment and does not isolate fortification effects. The ordinary slot
reached population 216 versus 289 in the original baseline, despite more Town Centers
and defences; this run does not improve every case.

The 15.01-second narrated clip `.cache/showcase/ai-fortifications-current.webm`
shows actual forest-slot gameplay at 55:33–55:47, including the resource towers.
It was visually verified at the beginning, middle and end. Earlier white/loading
takes and the three-second prototype clip are not the attachment.

## Economy recovery measurement

Same recipe 4 France package, seed 1 and four validated anchors for both runs.
Before: `7c9ba8e49daf6f36b2b8922ea7aabc3f64f25b34` in a frozen checkout.
After: defences follow local danger and economy reserves preserve growth; the JSON manifest identifies sources.
The historical recipe 3 results above are a separate comparison, not this control.

Open land reaches 876 population versus 29 in the current control and the original target 289.
Every slot improves against the current control. The historical recipe 3 island result
was 1,053 versus 945 here; that older result is not an identical-input comparison.

Castle stone waits for Castle Age; known timber can receive camps out to 256 tiles.
Quiet Vassals release builders, and galleys require a nearby enemy warship.
Unfunded optional science reserves only owned resources and leaves surplus food for growth.

| minute | source | slot / case | pop | TCs | cells | age | techs | food / wood / gold / stone | military | kills | castles / towers / walls / gates | Stewards / Vassals / Generals |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 30 | before | 0 / island | 64 | 1 | 2 | CASTLE | 4 | 376 / 45 / 30 / 158 | 29 | 0 | 1 / 1 / 0 / 0 | 0 / 0 / 0 |
| 30 | before | 1 / forest | 56 | 1 | 1 | CASTLE | 5 | 470 / 60 / 645 / 215 | 19 | 0 | 1 / 0 / 0 / 0 | 0 / 0 / 0 |
| 30 | before | 2 / forest | 52 | 1 | 1 | CASTLE | 7 | 270 / 7 / 651 / 130 | 19 | 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 |
| 30 | before | 3 / land | 31 | 1 | 2 | FEUDAL | 2 | 3567 / 9 / 1841 / 1088 | 1 | 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 |
| 30 | after | 0 / island | 80 | 2 | 3 | CASTLE | 4 | 21 / 203 / 235 / 449 | 32 | 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 |
| 30 | after | 1 / forest | 78 | 1 | 2 | CASTLE | 5 | 69 / 150 / 181 / 627 | 34 | 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 |
| 30 | after | 2 / forest | 85 | 2 | 2 | CASTLE | 11 | 3 / 170 / 66 / 395 | 42 | 0 | 0 / 1 / 0 / 0 | 0 / 0 / 0 |
| 30 | after | 3 / land | 56 | 1 | 2 | CASTLE | 7 | 508 / 5 / 355 / 370 | 18 | 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 |
| 60 | before | 0 / island | 253 | 4 | 4 | IMPERIAL | 5 | 325 / 66 / 25 / 526 | 150 | 0 | 2 / 3 / 0 / 0 | 0 / 0 / 0 |
| 60 | before | 1 / forest | 217 | 3 | 4 | IMPERIAL | 6 | 291 / 5 / 145 / 195 | 143 | 0 | 2 / 2 / 0 / 0 | 1 / 1 / 0 |
| 60 | before | 2 / forest | 179 | 2 | 3 | IMPERIAL | 8 | 1218 / 361 / 308 / 30 | 113 | 0 | 1 / 0 / 0 / 0 | 0 / 1 / 1 |
| 60 | before | 3 / land | 31 | 1 | 2 | FEUDAL | 2 | 6641 / 9 / 5191 / 1938 | 1 | 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 |
| 60 | after | 0 / island | 203 | 4 | 4 | IMPERIAL | 11 | 326 / 1004 / 1438 / 177 | 95 | 0 | 2 / 0 / 0 / 0 | 1 / 0 / 1 |
| 60 | after | 1 / forest | 183 | 2 | 3 | IMPERIAL | 6 | 71 / 842 / 191 / 1293 | 124 | 1 | 1 / 0 / 0 / 0 | 1 / 0 / 0 |
| 60 | after | 2 / forest | 242 | 4 | 4 | IMPERIAL | 21 | 1288 / 842 / 377 / 685 | 136 | 0 | 2 / 1 / 0 / 0 | 1 / 0 / 1 |
| 60 | after | 3 / land | 196 | 3 | 4 | IMPERIAL | 8 | 31 / 60 / 57 / 359 | 112 | 0 | 2 / 0 / 0 / 0 | 0 / 0 / 2 |
| 120 | before | 0 / island | 633 | 7 | 4 | IMPERIAL | 47 | 140112 / 9 / 1469 / 98 | 314 | 0 | 3 / 5 / 0 / 0 | 4 / 3 / 3 |
| 120 | before | 1 / forest | 1112 | 12 | 9 | IMPERIAL | 53 | 47219 / 67294 / 1095 / 90 | 491 | 5 | 3 / 6 / 93 / 3 | 8 / 3 / 3 |
| 120 | before | 2 / forest | 1040 | 10 | 9 | IMPERIAL | 53 | 14767 / 11616 / 147 / 4 | 635 | 6 | 4 / 16 / 284 / 8 | 4 / 4 / 3 |
| 120 | before | 3 / land | 29 | 1 | 2 | FEUDAL | 2 | 13166 / 9 / 5601 / 1938 | 1 | 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 |
| 120 | after | 0 / island | 945 | 7 | 4 | IMPERIAL | 53 | 94960 / 0 / 1453 / 103 | 444 | 0 | 3 / 5 / 14 / 1 | 6 / 3 / 3 |
| 120 | after | 1 / forest | 1395 | 22 | 13 | IMPERIAL | 53 | 39354 / 102696 / 94 / 51 | 657 | 2 | 3 / 20 / 222 / 5 | 7 / 3 / 2 |
| 120 | after | 2 / forest | 1667 | 16 | 11 | IMPERIAL | 53 | 82690 / 12313 / 777 / 87 | 988 | 0 | 5 / 15 / 299 / 7 | 8 / 4 / 5 |
| 120 | after | 3 / land | 876 | 8 | 6 | IMPERIAL | 53 | 75741 / 2 / 55 / 38 | 322 | 0 | 2 / 4 / 0 / 0 | 7 / 2 / 2 |

| source | wall seconds | timing records | tick p50 / p95 / max (us) | applied orders |
| --- | --- | --- | --- | --- |
| before | 2861.32 | 144001 | 4653 / 19565 / 1491201 | 35703 |
| after | 3431.08 | 144000 | 6080 / 29805 / 1482843 | 60851 |

| source | slot | CPU ms/decision | slowest CPU ms | slowest wall ms | max orders/decision | orders | SDK losses |
| --- | --- | --- | --- | --- | --- | --- | --- |
| before | 0 | 16.33 | 197.03 | 256.89 | 13 | 6367 | 0 |
| before | 1 | 41.63 | 223.39 | 383.52 | 16 | 13510 | 2 |
| before | 2 | 37.28 | 178.23 | 214.34 | 17 | 15281 | 41 |
| before | 3 | 1.11 | 143.32 | 143.33 | 8 | 549 | 14 |
| after | 0 | 16.74 | 97.32 | 129.19 | 14 | 9452 | 0 |
| after | 1 | 29.21 | 242.26 | 243.86 | 17 | 20612 | 0 |
| after | 2 | 31.18 | 185.50 | 186.81 | 21 | 20579 | 18 |
| after | 3 | 32.53 | 239.54 | 259.75 | 18 | 10215 | 2 |

Evidence: `.cache/ai-scaling/ai3-before.json`, `.cache/ai-scaling/ai3-growth.json` and
their adjacent timing streams. The frozen baseline original is in the local
`AoeWorld-ai3-before` checkout. Both source manifests and validated starts are retained.
The intentionally interrupted threat-only prototype (`ai3-threat-budget.json`) is
incomplete and excluded. The timber prototype (`ai3-timber.json`) retains its
own measured results but predates the optional-research reserve correction.
Setup failures before any game ticks are excluded.

These shared-host lockstep runs do not qualify live tick rate, dedicated hardware,
the scale ladder, defensive efficacy, transport attacks or force projection.
Network delivery can change decisions; this is one paired workload, not an ablation.
No qualifying attack video was captured. Full suites and CI were not run here.
