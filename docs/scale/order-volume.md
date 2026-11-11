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
  --label orders-before-current --no-summary --out .cache/order-volume/before-current.json" make scale-match
```

The copied map store already contains the same verified saved pair. Select
that stored save directly: the importer deliberately refuses occupied names.
For the second run keep the save path, change the label to `orders-after-current`
and the output to `after-current.json`.
Both runs start from the same input. Wall-clock AI decisions and shared-host
load vary; this is neither dedicated-hardware qualification nor a controlled
causal timing benchmark.

## Current qualification status

The functional code is `3dd22b72cfd95fd980f4c8dee07dc8377220e903`, rebased
on dev `82f7a3bf98d3903ab8738d5c71801ba9222e2c80`. The required fresh
owner-save comparison and instruction comparison are **blocked and unqualified**.
Neither the requested 50% reduction nor the one-percent instruction limit has
been demonstrated. The older `before.json` / `after.json` experiment used
superseded revisions, lacked the new ten-second counter and mover endpoints,
and included rejected behavior. Its figures are not reused here.

The measurement baseline is instrumentation-only revision
`f78fec1d7de604849e8c8c7212b2127d30c6a74f` plus the diagnostic patch
`.cache/order-volume/measurement-counters-current.patch`, SHA-256
`f092c93c9a57baecb82827cad0ef3172be0843c49eeba752e544c8b8d63bda25`.
The applied diff has that same hash and changes only three diagnostic files;
its dirty status is explicit. Bare dev, without that patch, was selected for
the instruction control. No measured window started on either control.

| Required saved-match observation | Before | After |
| --- | --- | --- |
| Decisions, orders, near reissues, reassigned <10s, superseded and fresh searches/min, for every issuer | Unmeasured | Unmeasured |
| Near reissues + superseded <10s, per wall minute | Unmeasured | Unmeasured |
| Same sum per 1,000 simulation ticks | Unmeasured | Unmeasured |
| Units and movers at the initial/final endpoints | Unmeasured | Unmeasured |
| TPS and route-serve p50/p95 ms | Unmeasured | Unmeasured |
| Late-game authoritative-tick instructions | Unmeasured | Unmeasured |

Player, Python AI, auto-scout, Steward, Vassal, General and rally/automation
are all required in the issuer table once a fresh pair exists. Missing values
are unavailable, never zero. After measurements were not started while their
matching baseline was blocked by the shared slot.

Each command below was tried once, followed by useful independent work and
one delayed retry. All were refused before running, with Make exit 2 wrapping
resource exit 75. These load averages describe **refused commands**, not a
performance measurement window; all are from the shared twelve-core host.

| Last attempt, UTC on 2026-11-11 | Command | Host load, 1/5/15 min | Result |
| --- | --- | --- | --- |
| 06:14:10 | `NEXTEST_TEST_THREADS=2 RUST_TEST_THREADS=2 CARGO_BUILD_JOBS=2 make -j1 perf-late-game-instructions` on bare dev | 8.49 / 12.44 / 17.81 | Long slot busy |
| 06:25:26 | Five-minute `make scale-match` command above on the instrumented baseline | 8.61 / 13.25 / 15.62 | Long slot busy |
| 06:25:34 | `NEXTEST_TEST_THREADS=2 RUST_TEST_THREADS=2 CARGO_BUILD_JOBS=2 make preflight` on the feature branch | 8.25 / 13.09 / 15.56 | Long slot busy |

At all three last attempts the holder was `make match-bench` in the timers
worktree's `.cache/phase7b-before`, started at 06:10:05 UTC. Logs are
`dev-instructions-retry.log`, `before-current-retry.log` and
`preflight-retry.log` under `.cache/order-volume/`. Earlier refusals are also
retained. No lock, holder, gate, workload or baseline was changed to obtain admission.

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

All commands use `NEXTEST_TEST_THREADS=2 RUST_TEST_THREADS=2
CARGO_BUILD_JOBS=2`. These results qualify code revision `3dd22b72cfd9`;
later documentation-only commits do not change its behavior.

| Command | Result |
| --- | --- |
| `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'` | 1,292 passed, zero skipped |
| Corresponding full `aoe-server` command | 453 passed; four pre-existing package-dependent tests ignored |
| `make sdk-python-test` | 23 bindings tests and 163 AI tests passed |
| `make test-harness` | 1,509 unit tests and the integration binaries passed; one explicit profiling test ignored |
| `make test-harness-focused HARNESS_TEST_FILTER=perf_scale::scale_match` | 23 passed |
| `make hooks-install hooks-check fmt-check lint structure-check architecture-check docs-check build-wasm` | Passed |
| `make deny` | Passed |
| `make perf-wasm-cap` | Passed: optimized gzip 407,793 bytes, existing repository cap unchanged |
| Normal commit hooks | Passed |
| `make preflight` and instruction comparison | Refused by shared resource admission; not passed |

The ignored harness test is `late_game_apply_census`, which requests explicit
100-seat profiling through `make movement-apply-census`. The four server
ignores require local packages; one also requires a quiet machine. These and
CI remain outside this qualification. Logs include
`simulation-qualified-all.log`, `server-qualified-all.log`, `sdk-qualified.log`,
`remaining-short-gates.log`, `frontier-gates-client.log`, `frontier-wasm-cap.log`
and `frontier-commit.log` under `.cache/order-volume/`.

The unchanged five-variant General gate passes: engagement shares are
87.774%, 88.112%, 85.400%, 92.231% and 89.035% (mean 88.510%). Every original
wait limit passes. These are deterministic functional observations, not
wall-performance measurements. The latest full-suite reports are copied to
`.cache/order-volume/general-qualified/`.

The current late-game replay pin was regenerated for retained goals and exact
contacts. Clean dev passes the preceding `f06497a1…` pin; the prior contact
retention revision `9e8c616e0f53` and the withdrawal correction both produce
`49d847124f3ec329d72805ecd276d785c2baa1d6d765cba0cc7a4f17daef394d`.
The exact workload, all activity floors and complete one-/four-worker equality
remain unchanged. No engagement floor or performance gate was reduced.

## Fresh video evidence and capture limits

The paired VP9 WebM recordings show the same Steward fixture, initially twenty
villagers, with original art. Both are 1280×720 at 25 fps. Extracted frames at
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
