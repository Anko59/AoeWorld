# Steward economy: earlier stability measurements

The numeric before measurements below describe the former Castle-trained
Vassal. They are historical context, not measurements of the Town Center-trained
Steward. The current measurement test and command are named below.

The headless measurement lives in
`crates/simulation/src/game_world/tests/economy/steward/kingdom/long_run.rs`.
It runs twenty villagers for 12,000 ordinary 20 Hz ticks (ten game minutes),
starting with 900 Food, 600 Wood, 300 Gold and 200 Stone. Each later game minute
spends up to 300 Food and 200 Wood. Training stays off. The seeded `wood_realm`
fixture supplies a Town Center, Steward, Mill, Lumber Camp and real finite resources.

Run it through Docker with:

```sh
AOE_HEAVY_WAIT_MINUTES=5 make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(ten_minute_early_economy_keeps_workers_productive)'
```

The test writes its latest observations to the ignored local file
`.cache/steward-stability/long-run.txt`. The retained before observation is
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

`an_unclaimed_worker_never_falls_back_to_a_sibling_stewards_tree` provides an
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
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(ten_minute_river_stewards)'
```

The test writes `.cache/steward-stability/river.txt`; the local baseline is
`river-before.txt`. Production baseline revision is
`4272cf15583bfb25899f0dd50695504caee0e8ce`. The assertion
`river_steward_one_and_four_worker_replay_matches` compares all observations and
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
fixture's population and a replaced Steward still charged in the capture scene.
Setup now uses ordinary death bookkeeping before removal in the measurement,
and keeps/repositions the original waiting Steward in the capture. Fresh baseline
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
was regenerated. The focused Steward and economy benchmark selection passed
87 tests, including both long runs and the severe-shortage dispatch regression.

### Accepted: busy-job adoption across a territory boundary

The next comparison uses committed pacing revision `aa24e196ec597` as its
baseline. Adoption previously considered the worker's position without checking
the site of its existing job. A neighbouring Steward could claim a worker walking
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

All 89 focused Steward/economy tests pass, including the full-population
one/four-worker comparison and its unchanged checkpoint pin. The river test
now also requires zero sampled outside-territory targets. No replay pin was
regenerated for this change.

The territory capture fixture now disables fog and removes the military
opponents and Wolves inherited from its base scene. A new guard first failed
because farmers died before activation; it requires all forty to survive
three game minutes. The same fixture-only patch is applied to the capture
baseline at `aa24e196e`, leaving its Steward AI unchanged. Capture frames are
illustrative; the prepared-river test remains the quantitative comparison.

## Small construction teams

Fresh comparisons use production baseline `541b7a3f1a67`. Other merged engine
changes mean its original forty-worker river result is now 421 changes and
391 resources/minute; the earlier Vassal numbers above are not its baseline.
The existing workload, spending, finite trees and ordinary 20 Hz clock remain.
The extended fixture adds two domains at y=160, giving four same-player Stewards
and eighty villagers on the prepared river terrain. A startup variant inserts
four ordinary unfinished Mills, with a guard proving they are foundations.
It otherwise uses the same positions, supplies and measurement loop.

The first focused helper regression failed with 25 builders on one House.
It now checks House, Mill, Farm, Lumber Camp and Mining Camp teams, deliberately
giving distant villagers lower IDs than nearby ones. It also requires the
foundation to finish, and work elsewhere while an additional Mill builds.
That last case supplies the completed Mill required to unlock Farms normally.
An additional partial-census regression first recruited six builders for a
three-person House team. It puts 1,200 soldiers between census slices, then
checks that the next slice includes the already managed builders in its count.
This bookkeeping adds at most the existing hundred managed workers to a helper
check, without enlarging the spatial census or map-search budgets.

### Trials, including rejected changes

All numbers below are fresh before → after comparisons against `541b7a3f1a67`.
Changes count resource-kind changes, same-kind target changes and construction
starts. Idle is the mean number of idle villagers, not a percentage.

| Forty-worker mature river trial | Job changes | Resources/min | Mean idle |
|---|---:|---:|---:|
| Three builders, eight-tile cutoff; rejected | 421 → 419 | 391 → 391 | 0.069 → 0.069 |
| Above plus Farms during Mill work; rejected | 421 → 419 | 391 → 391 | 0.069 → 0.069 |
| Remove cutoff; rejected | 421 → 421 | 391 → 391 | 0.069 → 0.069 |
| Two builders; rejected | 421 → 428 | 391 → 398.5 | 0.069 → 0.070 |
| Two builders, serialize camp construction; rejected | 421 → 409 | 391 → 350.5 | 0.069 → 0.069 |
| Three House/Mill builders, four camp builders; accepted | 421 → 421 | 391 → 391 | 0.069 → 0.069 |

| Eighty-worker mature river trial | Job changes | Resources/min | Mean idle |
|---|---:|---:|---:|
| Three builders, eight-tile cutoff; rejected | 736 → 732 | 730 → 731 | 0.138 → 0.136 |
| Above plus Farms during Mill work; rejected | 736 → 732 | 730 → 731 | 0.138 → 0.136 |
| Remove cutoff; rejected | 736 → 736 | 730 → 730 | 0.138 → 0.138 |
| Two builders; rejected | 736 → 736 | 730 → 730 | 0.138 → 0.138 |
| Two builders, serialize camp construction; rejected | 736 → 736 | 730 → 730 | 0.138 → 0.138 |
| Three House/Mill builders, four camp builders; accepted | 736 → 736 | 730 → 730 | 0.138 → 0.138 |

| Eighty-worker unfinished-Mill startup trial | Job changes | Resources/min | Mean idle |
|---|---:|---:|---:|
| Two builders; rejected | 895 → 772 | 551.5 → 658.5 | 0.209 → 0.155 |
| Two builders, serialize camp construction; rejected | 895 → 754 | 551.5 → 678.5 | 0.209 → 0.158 |
| Three House/Mill builders, four camp builders; accepted | 895 → 789 | 551.5 → 667.5 | 0.209 → 0.175 |

The initial startup draft changed a completed Mill's work counter without
changing its construction state. It did not exercise startup and is excluded
from that table; ordinary foundation insertion replaced it before comparison.
Startup was not measured for the first three trials.

The first trials failed existing territory or early-income tests. Removing
the cutoff did not restore early income: a team cap could leave a worker
buying another camp before the first opened. Serializing camps then lost
10.4% of mature-river income and failed the manual-stop adoption regression,
so it was removed. The accepted four-person camp team preserves the existing
early-income floor and ordinary late-game camp rebuilding. No test floor,
gate, replay pin or performance baseline was lowered.

### Accepted result and replay scope

Small teams reduce startup job changes by 11.8% and raise delivered resources
by 21.0%. Construction starts fall 159 → 55 and walking worker-ticks fall
628,919 → 576,064. All ten idle, disconnected-target and outside-territory
samples are zero. Mean idle falls as shown above. The two mature workloads
stay unchanged; the improvement is construction startup, not productive
resource rebalancing or exhausted-Dock management.

The original twenty-worker flat workload now also records the river test's
complete job-change definition, leaving its older movement/resource counters
intact. Its secondary result is a small regression, not a universal churn win:

| Single-domain metric | Before | After |
|---|---:|---:|
| Total job changes, ten minutes | 264 | 265 |
| Movement starts/destination changes per minute | 70.6 | 71.2 |
| Resource-kind changes per minute | 18.0 | 18.7 |
| Delivered resources per minute | 204.5 | 204.5 |
| Mean idle / final idle | 0.0421 / 0 | 0.0413 / 0 |

The accepted construction-startup result does not resolve that one additional
flat-workload job change. Productive Farm/tree continuity remains separate work.

The tests require startup churn and construction starts to fall, income no
worse than 3% down, walking and mean idle no higher, bounded sampled idle ending
at zero, and zero sampled disconnected/foreign-domain targets. Mature-work
churn and walking retain stricter current bounds alongside the original floor.

Each river case compares complete observations and ten canonical hashes from
independent one- and four-worker runs before checking its numeric acceptance:
`river_steward_one_and_four_worker_replay_matches`,
`four_domain_river_one_and_four_worker_replay_matches`, and
`four_domain_mill_startup_one_and_four_worker_replay_matches`.
No replay pin is regenerated. Local raw trials and commands are retained in
ignored `.cache/churn/`; real-art recordings belong in `.cache/showcase/`.
These are synthetic prepared river geography and bounded economy observations,
not source-backed France, the owner's 99-AI game, or hardware qualification.

## Productive-job continuity (2026-11-04)

Comparison base: `6525255562e330a930dae0a339a564f640c03e79` (small crews).
Kingdom reserve changes now choose available workers; they do not interrupt
productive gathering. An exhausted tree uses ordinary local replacement before
its woodline ends. Ordinary Farm reseeding remains enabled. A worker borrowed
for automatic support construction retains its original target, then resumes
through the admitted Steward order budget. Completed camps do not overwrite
that saved target with their own default resource search. Loaded managed
workers cannot be borrowed. Held Farm reservations prevent another helper from
claiming the absent farmer's plot; owner overrides, release and death clear them.
The bounded held-job map participates in the canonical hash.

### Trials, including rejected drafts

Each cell is **job changes / delivered resources per minute / mean idle** over
10 minutes. Columns reuse the twenty-worker flat workload and the prepared
river workloads above. Idle means worker-ticks divided by 12,000, not an idle
percentage. The startup fixture begins with four unfinished Mills.

| Variant | Flat 20 | River 40 | River 80 | River startup 80 |
|---|---|---|---|---|
| Base | 265 / 204.5 / .0413 | 421 / 391 / .0690 | 736 / 730 / .1383 | 789 / 667.5 / .1750 |
| Every depleted node hands off; rejected | 52 / 226 / .0774 | 75 / 616 / .1301 | 148 / 1250 / .2470 | 208 / 1144 / .2654 |
| Whole woodline, early Farm retirement; rejected | 91 / 218.5 / .0548 | 143 / 654.5 / .0657 | 279 / 1305.5 / .1290 | 299 / 1202 / .1677 |
| Ordinary reseeds, local construction capture; rejected | 91 / 218.5 / .0548 | 143 / 654.5 / .0657 | 279 / 1305.5 / .1290 | 299 / 1202 / .1677 |
| Shared construction capture, before completion guard; rejected | 81 / 235.5 / .0593 | 143 / 654.5 / .0657 | 279 / 1305.5 / .1290 | 299 / 1202 / .1677 |
| Shared capture and budgeted original-job resumption; accepted | 81 / 235.5 / .0593 | 140 / 655.5 / .0643 | 278 / 1306.5 / .1295 | 294 / 1215 / .1793 |

The first draft exceeded the unchanged startup idle ceiling: 3,185 worker-ticks
against 2,508. Retiring spent Farms before admitting an alternative also lost
useful prepaid reseeds when ore was refused. Both changes were removed. The
next draft captured Mill/camp construction but missed Dock construction. Moving
capture to ordinary automatic construction covered that path. Its completion
path still needed a guard against taking the building's default resource job
or continuing to another foundation instead of restoring the original target.
The final draft adds that guard. The held-Farm lookup was subsequently changed
to a precomputed reservation set. Its focused rerun preserved the complete
observations, including every replay hash, byte for byte.

Accepted job changes fall 69.4%, 66.7%, 62.2% and 62.7% respectively. Total
income rises 15.2%, 67.6%, 79.0% and 82.0%. Startup mean idle rises slightly
against the small-crew base, but remains below the existing .209 ceiling; every
minute sample and final idle count is zero in all river workloads. Flat mean
idle rises from .0413 to .0593, with final idle still zero. No gate or baseline
was reduced.

### Resource mix and limits

Keeping jobs changes the resource mix. These are actual deliveries per minute,
listed as Food / Wood / Gold / Stone; stockpile growth is not used as income.

| Workload | Before | After |
|---|---|---|
| Flat 20 | 96.5 / 108 / 0 / 0 | 119.5 / 116 / 0 / 0 |
| River 40 | 149 / 242 / 0 / 0 | 39.5 / 616 / 0 / 0 |
| River 80 | 251 / 479 / 0 / 0 | 75.5 / 1231 / 0 / 0 |
| River startup 80 | 233.5 / 434 / 0 / 0 | 283 / 906 / 16 / 10 |

The mature river workload becomes heavily Wood-oriented. Higher total income
does not establish a balanced kingdom economy when workers are fixed and
training is disabled. Training, purchases, combat and the owner's 99-AI game
remain separate qualification. This change preserves paid Farm tiling,
reseeding and camp repair; it does not claim to resolve same-player poaching,
all inaccessible resources, growing remote idle groups, or exhausted/far Docks.

Tests now reflect the owner's continuity rule. The old discovery cases expected
productive fallback farmers to be automatically retasked onto ore. Explicit
owner Stops or a finished fixture crop now supply idle opportunities, retaining
the original deposit floors, 10,000/4,000-tick deadlines, refused-order load
preservation, finder budgets and replay comparisons. New checks retain real
productive jobs through moderate and urgent stock changes, preserve delivered
loads, continue a local woodline, switch after its exhaustion, restore borrowed
workers, and honor a manual move during construction. The original twenty-worker
blocked-route fixture is retained for long-run measurement; clear delivery
routes isolate productive continuity in the stock-change regressions.

Local evidence is ignored under `.cache/churn-continuity/`. All river assertions
still compare complete one- and four-worker observations before numeric gates:
`river_steward_one_and_four_worker_replay_matches`,
`four_domain_river_one_and_four_worker_replay_matches`, and
`four_domain_mill_startup_one_and_four_worker_replay_matches`.
`late_game_full_population_replays_at_the_profile_checkpoint` also passes its
existing pin; no replay pin was regenerated. `make perf-late-game-instructions`
was refused with Make exit 2 / recipe exit 75: the `game/roads` checkout held the
long-job lock. No instruction result for this change is claimed.

Real-art before/after capture uses `make showcase`, the ordinary kingdom demo
and matching Scout cameras. Raw voiced captures are
`.cache/showcase/continuity-before.webm` and `continuity-after.webm`.
Delivery excerpts are `steward-continuity-before.webm` and
`steward-continuity-after.webm` in that directory. Initial loading is excluded
from excerpts. First, middle and last frames are inspected for both raw and excerpt
captures; footage is local evidence, not a CI gate.

Final local validation: `make hooks-install`, `make hooks-check`,
`make fmt-check`, `make lint`, and `make structure-check` pass. The final
`make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation` run with
`AOE_NATIVE_TEST_FILTER='test(steward::)|test(gathering::)|test(construction::)|test(late_game_full_population_replays_at_the_profile_checkpoint)'`
covers 203 gathering/construction/Steward tests and the unchanged-pin checkpoint
named above. The final guarded lookup preserves the recorded observations. No full suite, published CI result or hardware qualification is
claimed. Both delivered excerpts are 12 seconds, showing the settled Scout
view: the before HUD has one Food worker; the after HUD has eighteen.
