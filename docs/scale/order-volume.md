# Owner-save movement order volume

The workload is a copy of `1794176817-first.save` (9,395 units, 10,581
buildings, 99 AI opponents), SHA-256
`c51582bf7eb4007ac5cdb72ef2ab3a733c7a451532b6b41d6816a2fc9c85c11a`.
The original is read-only. Inputs, full reports and recordings live under
ignored `.cache/order-volume/`; game assets and recordings are not Git inputs.

## Counter definitions

`GET /admin/match/routes` exposes cumulative `order_demand.issuers` for player,
Python AI, auto-scout, Steward, Vassal, General and rally/automation.
`decisions` counts source orders: every external command (including training,
research and settings), manager order attempts, and automatic trip starts.
Steward and General decisions use their existing order budgets; Vassal and
auto-scout decisions are counted at their issuing sites. Automatic trip starts
are unit-granular; formation commands from other sources count once.

The remaining counters count **unit movement intents**, including task movement and each member
of a formation, rather than protocol messages (one message can move many units).
Training, research, settings and repeated attacks that never request movement
are counted in `decisions`, but excluded from the trip counters. `scale-match` separately retains the server's
protocol command totals in its tick records.

- `orders`: movement requests, including redundant and refused requests once
  the unit and destination have been admitted to movement validation.
- `near_reissues`: requests within one tile (Euclidean distance, fixed-point)
  of the unit's current active goal, including a shared formation goal.
- `reassigned_10s`: accepted replacements within ten simulation seconds of
  that unit's previous accepted movement request, including completed trips.
- `superseded`: accepted movement requests or explicit Stop replacing unfinished
  movement, charged to
  the **previous** issuer. The first replacement of a restored order has no
  measured previous issuer and is excluded.
- `superseded_10s`: the subset replaced before ten simulation seconds elapsed
  since the previous accepted request, charged to that previous issuer. The
  requested redundancy sum is `near_reissues + superseded_10s`. These
  counts can overlap; the sum is an indicator, not a unique-trip count.
- `route_requests`: route-admission calls, including cached outcomes and
  slot refusals. A formation requests one shared route.
- `route_searches`: fresh planners created at route issue, excluding cache
  hits. Route continuation, frontier exploration and read-only probes are
  separate work and are not counted as new issue-time searches.

A synchronous issuer context surrounds external command application and each
manager pass. Other automatic trips (gathering, combat pursuit, rally, etc.)
fall into rally/automation. Diagnostics are absent from canonical hashes and
saves; per-unit history is discarded on death. They never drive decisions.
`scale-match` retains an initial sample and each wall-minute sample so rates
use counter **deltas**, excluding restore and AI connection time.
Samples also retain map-unit counts (including resident corpses) and movers
whose public state is moving or planning. These are distinct from scheduler
carriers: one formation carrier can represent many moving units.

## Reproduction

```sh
SCALE_MATCH_ARGS="--save .cache/order-volume/maps/saves/1794176817-first.save \
  --minutes 5 --port 8185 --package-directory .cache/order-volume/maps \
  --label orders-regions-before --no-summary --out .cache/order-volume/resumed/regions/before.json" make scale-match
```

The copied map store already contains the same verified saved pair. Select
that stored save directly: the importer deliberately refuses occupied names.
For the current candidate keep the save path, change the label to
`orders-regions-after2` and the output to
`.cache/order-volume/resumed/regions/after2.json`.
For a repetition, choose unused labels and output paths; retain existing evidence.
Both runs start from the same input. Wall-clock AI decisions and shared-host
load vary; this is neither dedicated-hardware qualification nor a controlled
causal timing benchmark.

## Current qualification status

The functional revision is `c1675e9ffb2be07d4a5904316c70b242db75cefd`,
based on frozen dev `7e8ca5a6eb559de4d7e82ed183afe7b3750cd542`.
The required redundant-order reduction and current-revision instruction gate
remain unqualified pending the fresh candidate measurements. Current full
simulation/server, bindings, harness and client-size checks pass; SDK AI
gameplay is still running. No current preflight pass is claimed.

The measurement baseline is instrumentation-only
`5f76887e3dda75b9d63660ea5410a1b0283f4fb8`, plus the explicitly dirty,
four-file diagnostic patch
`.cache/order-volume/resumed/regions/baseline-instrumentation.patch`, SHA-256
`e34b383df570c1d25e6e5cf36a88fda3254fdfaef7cf86a77157dc5a4dfb40a4`.
The applied diff has the same hash. It adds the ten-second subset and endpoint
counts to the original instrumentation and renames its private formation
accessor to avoid a new upstream API collision; it changes no gameplay rule.

The fresh baseline completed 300.463 wall seconds and 5,127 simulation ticks:
1,800.222 redundant requests/minute, or 1,758.338 per 1,000 ticks. The first
candidate, `f8909ed10b7d`, completed 301.240 seconds and 4,172 ticks:
1,384.278/minute and 1,665.868 per 1,000 ticks. That pair **fails** the target:
23.105% lower per wall minute and only 5.259% lower per 1,000 ticks. Its lower
TPS is not evidence of an improvement. Raw reports and the rejected pair are
preserved as `before.json`, `after.json` and `pair-analysis.json` under
`.cache/order-volume/resumed/regions/`. The new candidate must independently
meet the target; old before/after experiments are not substituted.

### First pair: target missed, historical candidate

The table is the first `f8909ed10b7d` comparison, not a measurement of the
current `c1675e9ffb2b` candidate. Mean host load is adjacent to all rates;
endpoint loads and final rolling latency are recorded separately below.

| Window | Seconds / ticks | TPS | Units start→end | Movers start→end | Redundant/min | Redundant/1,000 ticks | Fresh searches/min | Mean host load 1/5/15 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| before | 300.463 / 5127 | 17.064 | 9351→9704 | 4087→1808 | 1800.222 | 1758.338 | 9959.629 | 6.20/16.04/19.75 |
| after | 301.240 / 4172 | 13.849 | 9351→9674 | 4091→1957 | 1384.278 | 1665.868 | 7891.382 | 13.52/11.09/15.02 |

Rates below are per wall minute; supersession windows are simulation seconds.

| Issuer / window | Decisions | Orders | Near reissues | Reassigned <10s | Superseded | Superseded <10s | Fresh searches | Mean host load 1/5/15 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| player / before | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 6.20/16.04/19.75 |
| player / after | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 13.52/11.09/15.02 |
| python_ai / before | 1904.660 | 2368.545 | 47.327 | 635.419 | 197.495 | 121.612 | 1072.944 | 6.20/16.04/19.75 |
| python_ai / after | 1691.807 | 2121.830 | 98.592 | 463.883 | 142.611 | 75.687 | 894.702 | 13.52/11.09/15.02 |
| auto_scout / before | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 6.20/16.04/19.75 |
| auto_scout / after | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 13.52/11.09/15.02 |
| steward / before | 411.765 | 322.502 | 0.200 | 244.423 | 61.505 | 47.726 | 292.549 | 6.20/16.04/19.75 |
| steward / after | 322.666 | 255.145 | 0.199 | 192.206 | 47.205 | 36.848 | 231.443 | 13.52/11.09/15.02 |
| vassal / before | 235.636 | 40.737 | 0.200 | 25.760 | 12.181 | 2.396 | 40.737 | 6.20/16.04/19.75 |
| vassal / after | 178.662 | 33.063 | 0.000 | 20.914 | 8.365 | 1.394 | 33.063 | 13.52/11.09/15.02 |
| general / before | 828.921 | 744.651 | 94.454 | 496.234 | 498.431 | 440.520 | 112.626 | 6.20/16.04/19.75 |
| general / after | 266.100 | 493.759 | 97.397 | 326.650 | 333.422 | 301.952 | 52.583 | 13.52/11.09/15.02 |
| rally_automation / before | 18114.443 | 18114.443 | 281.366 | 7158.752 | 887.430 | 764.420 | 8440.773 | 6.20/16.04/19.75 |
| rally_automation / after | 14543.885 | 14543.885 | 235.626 | 5510.224 | 627.008 | 536.582 | 6679.591 | 13.52/11.09/15.02 |

| Window | First sampled load 1/5/15 (UTC) | Last sampled load 1/5/15 (UTC) | Sampled load ranges 1 / 5 / 15 | Final rolling route-serve p50/p95 ms |
| --- | --- | --- | --- | --- |
| before | 8.80/22.05/22.13 (2026-11-11T17:09:29.190362+00:00) | 8.81/12.42/17.85 (2026-11-11T17:14:19.216964+00:00) | 4.34–8.81 / 12.33–22.05 / 17.85–22.13 | 7.089/19.436 |
| after | 8.11/9.75/15.36 (2026-11-11T17:18:50.351859+00:00) | 21.03/14.82/15.72 (2026-11-11T17:23:50.377812+00:00) | 8.11–24.01 / 9.38–15.05 / 14.47–15.80 | 7.235/21.714 |

Loads are ten-second observations on the shared twelve-core host. Preparation is excluded. Endpoint loads are observed within ten seconds of the measurement endpoints; the final rolling route-serve distribution is not a pooled five-minute latency distribution.

Observed reduction: 23.105% per wall minute; 5.259% per 1,000 simulation ticks.

The subsequent source probe at `6885ba341e62` attributed 2,872 redundant events
exactly to their callers. Its three largest pairs were hunting (604), combat
pursuit (530) and General squad marching (470). The source map reconciles both
flat counters exactly; it does not change their definitions. Mean sampled
host load (1/5/15 minutes) was 44.85/32.51/22.46. These are counts from a separate
diagnostic run, not the final comparison.

Bare frozen dev completed `make -j1 perf-late-game-instructions`: **183,199,043
instructions**, so the candidate must use at most **185,031,033** (one percent).
The control was admitted at 15:15:29 UTC on 2026-11-11 with host load
27.45 / 25.50 / 22.58 (1/5/15 minutes); active-benchmark load at 15:21:07 was
16.73 / 18.28 / 20.35, and completion load at 15:24:10 was
17.74 / 18.94 / 20.29. This is a synthetic authoritative-tick instruction
count, not an owner-save throughput result. The raw JSON's `Both[0]` is the
current count; its cached older `Both[1]` is not the candidate comparison.
Normal current-candidate instruction attempts at 20:03:57 and 20:08:53 UTC
were refused before execution by the external preflight holder, started
20:02:14 UTC. No current raw count was produced. No lock, workload or gate
was changed to gain admission.
The earlier `f8909ed10b7d` count of 178,850,645 passes for that revision only;
it does not qualify `c1675e9ffb2b`.

## Movement and worker rules


Area movement commands retain their route, search frontier, deadline and
queued waypoints when the active goal is within one tile. Identical contact
points are also retained; distinct precise interaction points remain distinct
even within a tile, so workers can reach the resource and soldiers can enter
attack range. Attack-move formation contacts also require exact equality.
An arrival point newly reserved by another task is selected again once planning
has completed. Soft reservations cannot discard a pending frontier or renew
its original admission deadline.
Both precisions use the same routing engine. Individual
overrides still leave formations; queued commands and attack-mode or formation
membership changes remain meaningful.

Hunting retains an admitted endpoint while it still reaches the moved prey.
Ranged pursuit likewise keeps a firing point while it remains within the
weapon's minimum/maximum range. Melee uses the original
contact-retargeting rule; its exact clash baselines remain unchanged. General
reinforcements receive their new formation orders without restarting soldiers
already marching to the same squad goal. New scalar commanders still enter
formation; changed goals and failed trips remain retryable.

General and Vassal sources retain unchanged army goals. Vassal return orders
still cancel a new pursuit. Auto-scout already selects its next goal only once
the current trip ends; a regression test covers arrival/failure behavior.
Wounded withdrawal compares the selected free arrival contact before issuing
an order. A retained trip rejoins its existing corridor through the ordinary
bounded local-refinement budget; its global search frontier and admission
deadline survive. Simply keeping an old local segment changed retreat timing
and failed the engagement gate. Repeating a live gather job preserves its
phase, work credits, partial loads and return trip; failed trips remain retryable.
Steward and Python AI productive rebalancing use a 20-game-second dwell and
leave workers in transit alone. Idle recovery and failed-job recovery retain
their existing prompt paths. Steward assignment tenure is canonical replay
state; diagnostic issuer history is not.

## Functional checks

These results refer to functional revision `c1675e9ffb2b` on the frozen base,
unless a row explicitly identifies historical evidence.
Commands use `NEXTEST_TEST_THREADS=2 RUST_TEST_THREADS=2 CARGO_BUILD_JOBS=1
CARGO_PROFILE_DEV_DEBUG=0` and the operational Make override
`RESOURCE_CAP='--memory=4g --memory-swap=4g --cpu-shares=1024 -e CARGO_BUILD_JOBS=1'`.
Only compiler concurrency is reduced; caps, optimization, runtime workers,
workloads and gate thresholds are unchanged. Development debug symbols are
omitted to reduce compiler memory. All compilation and checks run through Make.

| Command | Result |
| --- | --- |
| `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'` | 1,313 passed, no skips, 627.240 s |
| Corresponding full `aoe-server` command | 453 passed; four existing package-dependent tests skipped; 262.942 s |
| Focused order/General/replay checks | 11 current focused regressions passed; exact clash baselines, five General variants and three new retention tests; all also pass in the full suite |
| `make sdk-python-test` | Current 23 binding tests passed (83.34 s); full 163-test AI run in progress |
| `make test-harness` | 1,509 unit tests passed (459.96 s) plus all integration binaries; one existing explicit profiling ignore |
| `make hooks-install hooks-check fmt-check lint structure-check architecture-check docs-check deny` | Passed |
| `make build-wasm perf-wasm-cap` | Passed: optimized client gzip 407,730 bytes; diagnostics 90,396 bytes; unchanged cap |
| Normal integration commit hooks | Passed |
| `make preflight` | No current pass; admission has been busy |

The unchanged five-variant General gate passes with engagement shares
86.582%, 92.165%, 88.346%, 87.694% and 88.597% (mean 88.677011%). Every original
wait limit passes. Ten-minute Steward productivity and river-economy one-/four-
worker replay checks pass too. These are deterministic functional observations,
not wall-performance measurements. Reports are retained in
`.cache/order-volume/resumed/regions/ranged-general/`.

The current retained-goal/exact-contact replay pin is
`21b25c880ce6937001c72f41f9a591665e673c3908d8b6455d9ffa87891e331b`.
Clean dev independently passes its `d740cb40...` checkpoint. Before repinning,
the feature passed every activity floor and complete one-/four-worker equality,
then failed only the final old pin; the new pin subsequently passed both the
focused and full suites. Region proofs now settle the old sealed waiting
fixture immediately, so its test uses a connected detour, a nonempty pending
frontier and an explicit peer reservation instead. Hash, route-request count
and original-deadline assertions remain intact. No gate or floor was weakened.

Current logs are `final-simulation-corrected.log`, `final-server-all.log`,
`final-sdk-all.log`, `final-harness-all.log`, `final-wasm.log`,
`melee-contact-green.log` and `melee-contact-gates.log` under the resumed
`regions/` folder. `final-simulation-all.log` is the rejected earlier
all-contact implementation: its two exact melee clashes failed. The source
was corrected without changing those expected results; the full current
suite subsequently passed. Historical integration logs remain preserved.
The four server skips require local packages; one also requires a quiet host.
CI remains outside this local qualification.

## Paired worker footage and capture limits

The retained paired VP9 WebM recordings show the same Steward fixture, initially twenty
villagers, with original art. They were captured on the preceding `82f7a3bf` base and retained-goal code
`3dd22b72cfd9`, before the region-router rebase; they are not fresh evidence
for the current routing base. Both are 1280×720 at 25 fps. Extracted frames at
25/30 seconds show the farm camera and working villagers. They are fixture
footage, separate from owner-save measurements, and do not compare equal game
ticks, economic yield or performance. Baseline uses a release server; after
uses the debug server built by the passing SDK gate. Each uses its own client
build and the same art pack. Binary/client hashes and baseline diagnostic-patch
provenance are retained under `.cache/order-volume/`.

- Before: `.cache/showcase/orders-current-before.webm`, 35.96 seconds,
  SHA-256 `38403ada576ab6777f4e3a9f978958caf5dd3994c86a8bccc7d2c5f6ad6a614b`.
- After: `.cache/showcase/orders-current-after.webm`, 39.44 seconds,
  SHA-256 `4b6dea91a9de6c3d95fb0c8f80648d0fdbfe588f2825fa1b3c71997a02c316bb`.

Retained goals and withdrawal rejoining can change army trajectories and
retreat timing. Army footage could not be qualified on the baseline. The
existing focused `tests/proofs/ui-panel.spec.ts` General proof trained and
selected its General, then timed out in both Firefox/WebGL2 and Chromium
because the minimap parent intercepted its inner-canvas click, before a march
order. A separate recorder attempt using the supported camera path failed its
90-second view-settling requirement with zero pending/failed chunks. Neither
failure is treated as an order-regression result or a passing browser gate.
Tests, selectors and settling rules were not weakened; these unrelated helpers
were left outside the change. The after army proof was not run.

The proof command used `AOE_E2E_WORKERS=1 AOE_E2E_PORT_RANGE=8185-8185
AOE_E2E_SPECS=tests/proofs/ui-panel.spec.ts AOE_ASSET_PACK=local-assets/packs/54916fcb9c1e43c7a63aa486e3c47824b24ba94c887168ed210517273626bff6
make test-e2e`. Logs are `general-before-browser.log` and
`general-before-video.log`; failed browser artifacts remain in
`.cache/order-volume/general-before-ui-failed/`. The temporary opt-in request
was moved to `general-proof-config.json`, so it does not enable future runs.

Assets, inputs and recordings remain outside Git and images. Attach footage
with `SHIP_VIDEO` if a later session publishes; this task does not authorize
publication. Shared-host observations, synthetic functional workloads and
fixture footage do not establish finished RTS performance or dedicated-hardware
qualification. The branch remains unqualified for the requested performance
thresholds until resource admission permits the fresh comparison and preflight.
