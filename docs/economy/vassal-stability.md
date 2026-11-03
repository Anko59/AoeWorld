# Vassal stability measurements

The headless measurement lives in
`crates/simulation/src/game_world/tests/economy/vassal/kingdom/long_run.rs`.
It runs twenty villagers for 12,000 ordinary 20 Hz ticks (ten game minutes),
starting with 900 Food, 600 Wood, 300 Gold and 200 Stone. Each later game minute
spends up to 300 Food and 200 Wood. Training stays off. The seeded `wood_realm`
fixture supplies a Castle, Vassal, Mill, Lumber Camp and real finite resources.

Run it through Docker with:

```sh
AOE_HEAVY_WAIT_MINUTES=5 make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(ten_minute_early_economy_keeps_workers_productive)'
```

The test writes its latest observations to the ignored local file
`.cache/vassal-stability/long-run.txt`. The retained before observation is
`.cache/vassal-stability/before.txt`, measured on production revision
`52a3743442769513655d53d2f937d2593cac8369` with the measurement test added.
Its source hash is retained in `.cache/vassal-stability/workload.sha256`.

| Metric, all twenty villagers | Before |
|---|---:|
| Movement starts or destination changes per game minute | 71.00 |
| Resource-job changes per game minute | 21.70 |
| Share of villager ticks gathering | 47.57% |
| Mean idle villagers | 0.04 |
| Idle villagers at end | 0 |
| Delivered resources per game minute | 199.00 |
| Food / Wood / Gold / Stone delivered per minute | 83 / 116 / 0 / 0 |

Movement includes planning a new trip and changing a trip's destination.
Gathering counts authoritative Gathering, Farming and Fishing tasks. Idle
counts stationary, unplanned Idle, Carrying and Moving tasks without a builder
or garrison order. This includes workers waiting with a load or a stopped
Moving label. Delivery totals use real income credits, observed every tick
with the same bucket epoch on both sides, so spending and bucket expiry cannot
hide deliveries or manufacture income.

Every tick checks the existing order, resource-search, candidate, block,
placement, unit-census and building-census bounds. This synthetic workload
measures economy behavior; it does not qualify finished RTS performance,
the owner's France/99-AI match, or dedicated hardware.

## Territory regression

`an_unclaimed_worker_never_falls_back_to_a_sibling_vassals_tree` provides an
unreachable owned tree and a reachable sibling-owned tree. An ordinary manual
order proves the fallback can reach the sibling tree. The automatic issuing
pass must refuse that fallback before the worker receives a controller tag.
The original implementation failed this assertion; the issuing domain now
restricts replacements throughout the order, including before adoption.
