# Generated late-game performance workload

`make perf-late-game` profiles a generated economy without France packages,
original game assets, a browser or a listening port. `perf-smoke`, `perf-ci`
and `perf-full` include the identical workload. Shared-machine elapsed time
is informational; deterministic instructions and allocations gate CI.

## Recipe and activity

Seed 41 supplies a 16,384-square map with flat elevation, procedural forests
and other obstacles, plus a generated southern sea. One hundred Castle Age
seats start with 80 villagers and 60 military/support units each. Setup uses
the ordinary age-completion effects, including research bits and building
upgrades, before seeding mature stockpiles and housing.

Each seat has a full five-entry Town Center and Barracks queue and Loom
research in another Town Center. Twenty-five Barracks have occupied spawn
perimeters and a completed front entry: they really retry production each
tick. Other completion times are staggered. The headless driver replenishes
queues through normal train orders. Its order cost is reported separately.
Villager loads are staggered across ordinary gathering and return trips.
The bundled economy uses food, wood, gold and stone from the starting kit,
plus eight normal deposits on connected ground per mature village, so all 80 jobs per seat obey
target/work-tile reservations. Actor, queue and activity floors stay fixed.
The measurements below predate these bundled economy rules; this integration
does not qualify their current instruction count or shared-machine timing.

Armies receive 128–224-tile individual marches and a twelve-member Line
formation receives a 192-tile march. Each seat has an automatic explorer,
a Galley and Transport Ship moving on water, an injured Knight beside an
idle Monk, and an injured Archer garrisoned in a Watch Tower. The hundred
stored garrison occupants leave 13,900 map units after warm-up. No original
assets enter construction, execution, Git or container images.

Three independent worlds warm for 384 ticks, then record 128 headless ticks.
The same worlds then serve 128 real `GameplayService` ticks. These are
**successive windows**, not additive phases of one tick. One human and 99 AI
controllers join their real seats through production session handling and
subscribe to 192×128 cameras. Four pans per seat are staggered over the server
window. All 500 snapshots per run and every seat's final delta must arrive.
Transport channels are in process; wire validation and encoding occur outside
tick timing. `server_with_camera_tick` additionally includes camera admission
and snapshot construction; `server_tick` is the ordinary service tick.

Per-tick censuses reject collapsed displacement, marching, exploration,
formations, garrisons, naval population and production pressure. A completed
entry may leave a vacancy until the next normal refill; window endpoints
must have all thousand queued entries. Reports retain every census, canonical
checkpoint, raw phase sample, pooled p50/p95 and median of three run percentiles.
The minimum measured headless displacement was 6,912 units per tick, with at
least 4,396 marching military/support units, 496 returning workers and 25 mature queues.
The measured headless interval trained 173 units and deposited 4,620 wood.

Garrisons, Monk healing and tower acquisition contribute to `combat`;
formations, explorers and ships contribute to `movement`. Navy wire state and
fog also reach fan-out. This does not isolate every system's marginal cost:
there is no naval battle, tower siege, boarding or vassal workload. Vassals
are implemented in this bundle but are not exercised by this workload. The
ordinary wildlife path remains live.

## Calibration and deterministic baseline

The imported workload on the Castle Age tree measured 38.740/54.032 ms server
p50/p95, but only 0.453 ms headless research/training p50. It missed sustained production pressure, left 4,000 soldiers idle and used
short marches. The revised workload
measured 46.017/67.949 ms server p50/p95 before the new optimizations:
movement 25.127 ms, research/training 7.695 ms, fan-out wait 3.617 ms and
session views 1.354 ms (pooled server medians). Camera-inclusive p50/p95 was
47.057/70.267 ms. The owner's reported match was 46/114 ms, with movement
38 ms, research/training 11 ms, fan-out 7 ms and session views 3 ms.
The workload reproduces the dominant phase order and material production
pressure; it does **not** reproduce that two-hour match's 114 ms tail.

`aoe-scenario::LATE_GAME_RECIPE` binds the changed actors, routes and cameras
to workload identity. Its new current hash is
`36c2ec66e60a9bfbe2ab7aa381bdc9a5f3c7867b90a9620070955cab0c662a4d`.
The new warmed checkpoint is pinned in the workload test with its reason;
general movement and wire replay pins remain unchanged.

`make perf-baseline-propose` collects the complete authoritative tick after
the same warm-up under pinned Gungraun/Callgrind. One Rayon worker
makes counts independent of runner CPU count. Setup, warm-up and teardown are
outside the measured kernel; Callgrind instrumentation stays off during setup.
Only the changed late-game baseline is replaced from this unoptimized recipe's
proposal: 240,341,066 instructions, 4,127,477 allocated bytes and 4,733
allocations. The former late-game values describe a superseded, different
recipe; they cannot baseline this workload identity. Existing four kernel
baselines and the 5% allowance stay unchanged.
`make perf-late-game-instructions` repeats only its Callgrind kernel for focused
iteration. `make perf-late-game AOE_LATE_GAME_ARGS='--check --instructions'`
adds focused simulation/server/harness checks and the complete microbenchmarks
under one inherited heavy slot. No full native suite is run by that option.

Raw local before/after evidence lives under ignored `.cache/`; ordinary gate
reports live under `reports/perf/`. The generated workload does not qualify
real France terrain, a two-hour save, Firefox, a GTX 1660 Ti or dedicated
hardware. Navigation spans are inclusive and may overlap; do not sum them
with their parent movement span or sum phase medians into a tick percentile.

## Profiled hot paths

The calibrated tick's Callgrind trace attributes 68,859,913 instructions to
training and 141,268,524 to movement. Ordered active production sets remove
idle-building visits; spatial occupants replace whole-world spawn scans in
the same candidate order. Proven current walking segments can enter the
parallel proposal batch while future lookahead is pending. Formation and
mutating planner fallbacks keep their ordering and budgets. A route-cache
recency index replaces full-cache eviction scans without changing timestamp/
key ties or the entry/byte limits; its payload is charged to that byte limit.
Fan-out reuses bounded Rayon workers and builds ordered 4,096-unit frame parts.

On clean `d6efaf2517fd7994cb9185744395013147ccc983`, the complete tick fell
from 240,341,066 to 167,625,765 instructions (30.3%). Training fell to 865,875
and movement to 139,788,464 inclusive instructions. Seven route-cache inserts
fell from 3,094,589 to 18,997 inclusive instructions. These are one-worker
counts for the exact measured tick, not elapsed-time medians.

Elapsed server p50/p95 varied from 41.161/57.512 ms at `7afeb564` to
51.314/76.744 ms at `d6efaf25`, against the calibrated before 46.017/67.949 ms.
Unchanged phases also slowed; these shared-host samples do not establish a
controlled wall-time speedup. All three runs in each sample preserve the
before hashes, per-tick activity and complete server evidence. Raw phase
medians, tick percentiles and final gate comparisons are retained under
ignored `.cache/` and `reports/perf/`.

The first optimized CI attempt exposed a separate viewport instruction
regression: a non-inlined ordered-set insertion added 6,055 instructions,
raising 266,322 to 272,377 against the unchanged 270,472-instruction cap.
The query now collects matching entities directly and sorts/deduplicates by
id, preserving spatial work counters and output order while removing the
temporary tree. The existing baseline and threshold remain unchanged.
