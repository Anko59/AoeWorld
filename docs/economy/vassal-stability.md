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

## Two domains on river terrain

`long_run/river.rs` measures forty villagers in two overlapping same-player
domains for ten game minutes at 20 Hz. It uses the prepared-map river fixture:
real terrain navigation and collision, a six-tile river with a distant gap,
finite forests, ordinary Farms and drop-offs. The geography is synthetic; this
is not a source-backed France map or the owner's 99-AI match. Wolves are removed
before measurement to isolate economy decisions. Training stays off. Starting
stocks are 1,800 Food, 1,200 Wood, 600 Gold and 400 Stone; after the first minute,
each minute spends up to 600 Food and 400 Wood.

```sh
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(ten_minute_river_vassals)'
```

The test writes `.cache/vassal-stability/river.txt`; the local baseline is
`river-before.txt`. Production baseline revision is
`4272cf15583bfb25899f0dd50695504caee0e8ce`. The assertion
`river_vassal_one_and_four_worker_replay_matches` compares all observations and
ten minute-boundary canonical hashes from independent one- and four-worker
runs. No replay pin is substituted for that comparison.

Reassignments count resource-kind changes, same-resource target-tile changes
and every construction start, including initial construction employment.
Initial gather assignments and ordinary trips to deliver and resume the same
target are excluded. The three counts remain separate so natural
depletion does not masquerade as kingdom rebalancing. Walking share counts
villager ticks marked moving or planning, including stalled trips. Income uses
the same tick-exact delivery accounting as the earlier measurement.

Minute-boundary observations record idle villagers, unreachable gather targets
and gather targets outside their controller's territory. The independent
reachability oracle floods static tile components using terrain edge rules,
trees and building occupancy, excluding transient unit traffic. A target needs
a contact-neighbour tile in the worker's component. This detects disconnected
targets; it does not certify body clearance or diagnose temporary crowding.
All manager admission bounds are checked every tick.

Both this fixture and the capture scene assert that population equals their
living actors. An audit caught six removed actors still charged to the river
fixture's population and a replaced Vassal still charged in the capture scene.
Setup now uses ordinary death bookkeeping before removal in the measurement,
and keeps/repositions the original waiting Vassal in the capture. Fresh baseline
and candidate runs with the population guards reproduce every counter in the
tables below; their canonical hashes correctly change with the corrected state.
The baseline passes the independent one/four-worker comparison, then fails the
new churn-improvement assertion, as expected for unchanged production AI.

The baseline recorded 620 resource changes, 60 same-resource target changes and
21 construction starts: productive rebalancing accounts for 88% of the 701
changes, so it is the first rule investigated. Baseline income is 309.5
resources/minute, walking share 63.57%, and reassignments per villager per minute
1.7525. Idle counts at every minute boundary are zero (mean 0.0715); disconnected
target counts are zero. Two samples contain one out-of-territory target; that
observation remains a separate territory investigation, not a claim of no
poaching.

### Accepted: pace moderate productive rebalancing

The existing 5% improvement margin still selects the first productive change
in each domain pass. Further changes need a weighted reserve below a quarter
of the old one. The same condition applies to fallback kinds, so an unavailable
urgent resource cannot authorize a burst of moderate fallback changes. Failed
orders consume no reassignment allowance; loaded workers still deliver first.
The existing forty-worker severe-shortage/two-second regression still passes.

| River workload metric | Before | After |
|---|---:|---:|
| Reassignments / villager / minute | 1.7525 | 1.1925 |
| Resource-kind changes, ten minutes | 620 | 343 |
| Same-resource target changes | 60 | 110 |
| Construction starts | 21 | 24 |
| Moving/planning share | 63.57% | 57.06% |
| Delivered resources / minute | 309.5 | 369.0 |
| Food / Wood delivered / minute | 94.5 / 215 | 127 / 242 |
| Mean idle villagers | 0.0715 | 0.0704 |
| Minute-boundary idle villagers (all ten samples) | 0 | 0 |
| Minute-boundary disconnected targets (all ten samples) | 0 | 0 |
| Outside-territory targets, samples with a violation / peak count | 2 / 1 | 4 / 3 |

Reassignments fall 32%, income rises 19%, and walking/planning falls 6.51
percentage points. The measurement now requires at least 20% fewer job changes,
income no worse than 3% down, less walking/planning, bounded idle ending at zero,
and zero sampled disconnected targets against this production baseline.
The outside-territory observations worsen and remain unresolved; this slice
does not claim to fix poaching, helper selection, or Dock lifecycle.

The original single-domain workload was also rerun on the same baseline and
candidate, without changing its setup:

| Single-domain metric | Before | After |
|---|---:|---:|
| Movement starts/destination changes / minute | 57.3 | 76.0 |
| Resource-kind changes / minute | 16.3 | 16.0 |
| Gathering share | 39.23% | 52.62% |
| Delivered resources / minute | 163.5 | 216.0 |
| Mean idle / final idle | 0.0401 / 0 | 0.0419 / 0 |

This secondary workload increases movement starts alongside the 32% increase
in delivery, with little change in resource switching. The primary acceptance
is the multi-domain river comparison, not a claim that every movement count
falls on every map. Local observations are retained as `river-after-rate.txt`
and `flat-after-rate.txt` alongside their baseline files.

Unlike the earlier unaccepted draft, this change does not raise the first-job
margin from 105 to 175, add job timers, or change reachability admission. Earlier
trial results used a different production revision and are not comparisons
against this baseline:

| Earlier trial | Moves/min | Resource changes/min | Resources/min |
|---|---:|---:|---:|
| Earlier baseline | 71.0 | 21.7 | 199.0 |
| Territory only | 71.0 | 21.7 | 199.0 |
| Reachability only, rejected | 69.1 | 20.4 | 190.5 |
| Combined draft, rejected | 80.8 | 15.0 | 221.5 |
| Earlier final observation, rejected | 57.3 | 16.3 | 163.5 |

No earlier draft was applied. In particular, the combined draft's higher
income did not compensate for its extra movement, and the final observation's
lower movement did not compensate for lost income.

The existing full-population checkpoint also passed unchanged:
`late_game_full_population_replays_at_the_profile_checkpoint`, including
`assert_eq!(single, run(4))` before its current hash assertion. No replay pin
was regenerated. The focused Vassal and economy benchmark selection passed
87 tests, including both long runs and the severe-shortage dispatch regression.

### Accepted: busy-job adoption across a territory boundary

The next comparison uses committed pacing revision `aa24e196ec597` as its
baseline. Adoption previously considered the worker's position without checking
the site of its existing job. A neighbouring Vassal could claim a worker walking
to another domain's Farm or foundation. The focused regression first failed
with that neighbouring controller instead of an unclaimed worker.

The candidate admits a busy gatherer only when its resource/Farm target belongs
to this domain, and a builder only when the existing building belongs here.
It leaves the actual job and carried resources intact. A worker can be adopted
when it reaches its job's domain. The regression checks the whole ordinary trip
for both Farm and construction work, then adoption after actual arrival.

| River workload metric | Paced baseline | After adoption check |
|---|---:|---:|
| Reassignments / villager / minute | 1.1925 | 1.1500 |
| Resource-kind / same-resource changes | 343 / 110 | 337 / 99 |
| Construction starts | 24 | 24 |
| Moving/planning share | 57.06% | 55.75% |
| Delivered resources / minute | 369.0 | 380.5 |
| Food / Wood delivered / minute | 127 / 242 | 132.5 / 248 |
| Mean idle villagers | 0.0704 | 0.0723 |
| Minute-boundary idle / disconnected targets | 0 / 0 | 0 / 0 |
| Outside-territory targets, affected samples / peak | 4 / 3 | 0 / 0 |

This is a modest additional 3.6% reduction (17 fewer job changes), with 3.1%
more income. The independent one/four-worker river assertion passes again.
Territory samples inspect managed targets; temporarily unclaimed crossing
workers still count in all forty-worker churn, walking, income and idle
measurements. Zero sampled territory violations is not proof for every map or
for boundaries changing while a worker is already managed. The local raw
candidate observation is `river-after-adoption.txt`.

The single-domain observation is unchanged: 76 movement starts/minute,
16 resource-kind changes/minute, 216 delivered resources/minute, and zero final
idle. It is retained as `flat-after-adoption.txt`.

All 89 focused Vassal/economy tests pass, including the full-population
one/four-worker comparison and its unchanged checkpoint pin. The river test
now also requires zero sampled outside-territory targets. No replay pin was
regenerated for this change.

The territory capture fixture now disables fog and removes the military
opponents and Wolves inherited from its base scene. A new guard first failed
because farmers died before activation; it requires all forty to survive
three game minutes. The same fixture-only patch is applied to the capture
baseline at `aa24e196e`, leaving its Vassal AI unchanged. Capture frames are
illustrative; the prepared-river test remains the quantitative comparison.
