# Owner-save UI work

The HUD retains actor labels by entity id, updates reflected DOM properties
only when their live values change, and reads the minimap's size at startup
and resize instead of on every camera/actor update. The HUD's four-Hz cadence,
actor/minimap streams, content, command rules and original artwork are preserved.
The minimap owns a bulk Float64 copy of the typed actor stream instead of
boxing it through an iterator; the source may be reused or detached by WASM.
Build buttons share a per-HUD player, completed-building and builder snapshot,
reducing the completed-building scans from 25 to one when a builder is selected.
The style read for the bottom-panel boundary precedes HUD writes. A fresh
canvas geometry snapshot is shared by the HUD, building/transport panels and
Steward territory outlines before their DOM updates; resize refreshes it on the
next HUD update. No persistent viewport cache is added. Actors outside the exact
existing horizontal HUD bounds skip ground-height projection; selected actors
bypass this prefilter. Known unit states reuse the displayed pose lookup without
looking up the same unit again. Height only affects projected Y.

## Tests first

New browser regressions cover unchanged panel contents with no DOM mutations,
moving health/load labels retaining nodes while their contents change, removal
of dead actors' labels, and minimap actor updates without synchronous layout
reads. Dense typed-stream ownership, fresh build-menu state, and horizontal
projection equivalence at viewport edges, CSS scales and ground heights are
covered too. Each regression failed before its production fix. Existing minimap zoom,
selection and production tests also pass. Final gate details are recorded below.

The initial minimap test moved within one aggregated pixel; it was corrected
before implementation to cross a pixel boundary and reach its layout assertion.
Browser type/lint failures in the new tests were corrected with explicit missing
node/data checks. No gate, baseline or existing test was relaxed.

## Workload and provenance

Source base: `6356262841db7ac3d9dcfe1a4aebca8acbf64dbf`.
Private server: `/home/anko/Work/projects/AoeWorld-fps3-measure`, port **8194**.
Functional browser ports: **8196–8199**. The owner's port 8170 is untouched.
Server, simulation, protocol, core, map, assets and scenario source match this
checkout. The first candidate changes JavaScript only. The next candidate also batches
geometry reads in four client Rust modules.

The baseline and first candidate use this checkout's built WASM, SHA-256
`6626a67f85b923ecb32ccbfd90bc1be8d548e23aedf19fe225a4a05afe87f31c`.
The initial retained fps3 artifact had nine different rendering dependency
files. Its complete reference campaign is preserved under `.cache/ui/reference/`
and excluded from acceptance. Replacing that artifact is a real build change;
no unchanged-tree timing retry was performed.

Save: `1794451672-mekong.save`, SHA-256
`7ae8dcf6de393245ca1a9dcb86f60242fa909a41c1d06f5c2a6aea69c3b5f839`.
The original and private copy hash identically. Restore starts at tick 184362
with 11,756 units, 7,632 buildings, 99 opponents and eight AI hosts.
Save restoration does not resume 52 jobs; this limits equivalence to the owner's uninterrupted session.
Map: `2976b04b4d746a9bc8196194fb3a31a87ddfb1fd5878788edc4aa621a0d5c401`.
Original pack: `8f7abe8c330d7fd710daa076c8ef07de65f8973feed59b99f4d01f9cbc9c4530`.
All saves, assets, screenshots, profiles and recordings remain outside Git.

The previous fps5 measurement scripts are reused in `.cache/ui/` with paths
adjusted. Each campaign restores the same save before Firefox WebGPU normal,
Firefox WebGPU far, and Chromium WebGL2 normal. Each phase targets 180 seconds.
Viewport: 1920×1080 CSS pixels, DPR 1, capital (925,1277), zoom **0.25 / 0.16**;
both views draw detailed sprites. Normal pans every 30 seconds between the
capital and offsets (32,16), (-32,-16), (0,0), selecting every 120 seconds.
Far and WebGL2 stay at the capital with an owned actor selected.
Runtime: Firefox **157.0**, Chromium **156.0.8078.47**, Playwright **1.64.1**.
After the borrowed cache disappeared, Firefox was copied from the pinned
`aoeworld/browser-tools:1.64.0` image into this checkout's ignored cache;
its executable SHA-256 is
`1f637a7cfe8f1a3ec814a7da7e38c360a3e3c39553b8c7245ac5d6643fd495eb`.
The Firefox version remains the same across the replacement.

Every positive raw frame interval in each complete phase is included, including
stalls and the final drain. UI/stage values are medians of five-second windows'
p95, not additive parts of a frame percentile. A separate 45-second Firefox
profile diagnoses the far HUD; separate video recording does not overlap timing.
The frozen campaign and acceptance rules are in `.cache/ui/plan.json` through
`plan7.json`: Firefox p50 must improve at least 15% in both views, and WebGL2
normal must regress no more than 5%. Every changed tree is measured once.

## UI profile

Firefox WebGPU far, an owned Villager selected: 144 HUD calls in a separate
45-second diagnostic. Mean JavaScript durations are numeric HUD **10.23 ms**,
details **13.40 ms**, minimap actors **12.48 ms** (nested in details), and
building DOM updates **0.88 ms**. The wrappers also copy their inputs for a
same-payload visual check, so these are instrumented diagnostics, not frame
benchmark results.

The Gecko profile has 3825 HUD samples at a nominal two-ms sampling interval.
Estimated sampled time per HUD call is `samples × 2 / 144`; percentages are
inclusive shares of HUD samples and overlap, so these rows must not be added.

| Function / work | Sampled ms per HUD call | HUD share |
| --- | ---: | ---: |
| `village::hud::update` (whole stage) | 53.1 | 100% |
| `updateDetails` | 12.4 | 23.4% |
| minimap `actors` | 11.5 | 21.6% |
| HUD `numeric` | 8.9 | 16.8% |
| forced layout flushes | 7.7 | 14.6% |
| Rust `building_panel::update` | 7.0 | 13.2% |
| minimap `render` | 7.0 | 13.3% |
| `innerHTML` replacement | 5.9 | 11.2% |
| `map::screen_position` | 4.4 | 8.4% |
| `picking::enemy_building` | 4.0 | 7.5% |
| `picking::resource_target` | 3.3 | 6.3% |
| `picking::actor` | 2.3 | 4.4% |

The stable single selection does not rebuild the selection grid. Formatting
is included in numeric presentation; no independent formatting cost is claimed.
Minimap geometry reads and moving actor labels are the largest avoidable DOM
costs identified here. No minimap throttling or reduced HUD content is used.

## Results

Final single-campaign result, with all complete-phase frames and stalls:

| Renderer / view | Baseline p50 / p95 ms | Final p50 / p95 ms | p50 improvement | UI before → after ms |
| --- | --- | --- | ---: | --- |
| Firefox WebGPU normal | 113 / 172 | 100 / 137 | 11.5% | 46.5 → 26 |
| Firefox WebGPU far | 129 / 207 | 104 / 155 | 19.4% | 63 → 34 |
| Chromium WebGL2 normal | 161.2 / 256.7 | 125.3 / 191.7 | 22.3% | 71.1 → 35.25 |

**The final candidate does not qualify.** Normal Firefox improves 11.5%, below
15%; far improves 19.4%. The 50 ms target is also unmet. The four-hour stop rule
ends this session with the tested patch and this explicit performance shortfall.
Each comparison uses all three views from the same candidate and every
completed-phase frame.

Firefox WebGPU normal: 186.731 s, 1848 frames; actor range [176, 582], median window draw count 2063.
Firefox WebGPU far: 183.999 s, 1686 frames; actor range [826, 837], median window draw count 2623.
Chromium WebGL2 normal: 186.607 s, 1432 frames; actor range [420, 430], median window draw count 4489.5.

Earlier attempted trees, each measured once:

The first JavaScript-only candidate ran once in each required view:

| Renderer / view | Baseline p50 / p95 ms | First candidate p50 / p95 ms | UI before → after |
| --- | --- | --- | --- |
| Firefox WebGPU normal | 113 / 172 | 103 / 151 | 46.5 → 34.5 |
| Firefox WebGPU far | 129 / 207 | 109 / 179 | 63 → 43.5 |
| Chromium WebGL2 normal | 161.2 / 256.7 | 127.3 / 196.9 | 71.1 → 52.65 |

Normal improves 8.8%, far 15.5%, and WebGL2 normal 21.0%. Normal does not meet
the frozen 15% acceptance threshold, so that candidate does not qualify. The
complete journals and first-candidate recordings are retained. The next change
batches remaining Rust panel geometry reads ahead of DOM writes. Two additional
red/green browser cases observe all late canvas-size reads for Villager and Town
Center selections, including a resize; they pass with the geometry snapshot.

That second candidate records **129 / 180 ms** normal, **108 / 175 ms** far,
and WebGL2 normal **182.3 / 262.3 ms**. UI is **39 / 42 / 51.45 ms** respectively.
It does not qualify: normal frame time and WebGL2 regress despite the lower UI
cost. Host load reaches **32.7 on 12 logical CPUs**, then falls to about 14.
All observations are retained, with no timing retry. A prelaunch dependency
failure produced no browser launch/journal; the deleted borrowed Playwright and
Firefox copies were replaced from this checkout and the pinned browser image.
Firefox remains version 157.0.

The third candidate adds the bulk-owned minimap copy and per-HUD build-menu
snapshot. Firefox normal records **100 / 138 ms**, far **102 / 157 ms**, and
WebGL2 normal **120.1 / 205.5 ms**; UI is **27.5 / 34 / 28.4 ms**. Normal improves
11.5%, far 20.9%, WebGL2 25.5%. Normal still misses the acceptance threshold.
Its WASM SHA-256 is
`687900c085332547fedfbd87cc11df935a3e18b1f17ad6a9ec5fd2180834e6a5`.
The full results and attempted recording metadata remain under `.cache/ui/attempt3/`.

The fourth candidate skips impossible horizontal projections and redundant unit
lookups. Its WASM SHA-256 (also checked over HTTP from the private server) is
`94698cbfa055955446c4a305a62f96ea8ddeea1679398102067d0da058468edb`.
Its single campaign records **100 / 140 ms** normal, **102 / 152 ms** far,
and **142.6 / 230.4 ms** WebGL2 normal. UI is **27 / 34.5 / 40.1 ms**.
It also misses the normal threshold; all results are retained. The earlier second-candidate
WASM is `f1db8622a58ca7094f698c79501b92f8d738b5b3f0f863ad3d47542bdb35c085`.

The fifth candidate makes actor picking ask for ground height lazily. An exact
sprite-body hit returns immediately; a horizontal miss of both sprite and
small ground target skips projection. Other hit candidates still use the original
full ground and fallback-bounds test. Terrain-depth admission, target ordering
and the displayed pose stay unchanged. Its WASM regression compares the original
hit rule over zoom, body/fallback bounds and extreme vertical positions, and
requires zero projection callbacks for 10,000 horizontal misses. Its HTTP-verified
WASM SHA-256 is
`ef11f400c66a7dd42614cc23811415f4bf655f3c00f55cc90008d36bb4f2331c`.
The fifth campaign records **105 / 163 ms** Firefox normal, **101 / 151 ms**
far, and **154.4 / 234.3 ms** WebGL2 normal. UI is **29.5 / 32.5 / 41.2 ms**.
Normal again misses the threshold; full results and attempted recording metadata are retained under
`.cache/ui/attempt5/`.

The sixth candidate applies exact horizontal rejection to resource markers,
after updating selected-resource contents and nearest-resource hints. Highlights
always keep their full projection. The half-open viewport rule is checked over
all edges and extreme heights. The holdings overlay owns a bulk numeric snapshot
instead of boxing and joining a string signature on every HUD call. Its dense
20,000-row regression retains the same drawing count, source-reuse ownership,
change/reset behaviour, NaN equivalence and signed-zero equivalence; the old
signature visits 80,000 iterator values, and the new one visits zero.
Its HTTP-verified WASM SHA-256 is
`2553ea45af6d6f613e5811af87152a93bb2f51aff69c372c1b8b69834c29aa53`.
The sixth campaign records **102 / 149 ms** Firefox normal, **128 / 186 ms**
far, and **140.4 / 205.3 ms** WebGL2 normal. UI is **28 / 40 / 36.9 ms**.
Neither Firefox view meets the threshold. Full results and attempted recording metadata remain under
`.cache/ui/attempt6/`.

The seventh candidate guards unchanged details/cursor attributes and canvas
stance/rally attributes using their live DOM values. The HUD revision continues
to increment, and changed cursors still apply. Two new browser regressions fail
on repeated detail writes and live stance/rally rewrites before this change.
Final static/native/cap checks and all 36 focused cursor/HUD browser cases pass.
All 10 strategic transition cases also pass on this final build. Its HTTP-verified
WASM SHA-256 is
`5f337bd45dbd69e296bdf207267ddb56d559c3c3c5eb9deccbc3ceebc6ce5302`.
The final campaign result is the table above.

## Checks and limits

All compilation/checks use Dockerized Make, `CARGO_PROFILE_DEV_DEBUG=0`, and
`NEXTEST_TEST_THREADS=2` for focused native tests.

- `make hooks-install hooks-check`: pass.
- `make fmt-check lint structure-check`: pass.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-client AOE_NATIVE_TEST_FILTER='all()'`: 40 passed, zero skipped.
- `make perf-wasm-cap`: pass; optimized client gzip 412762 bytes versus unchanged 375952 baseline (+9.8%, unchanged +25% ceiling). The starting checkout build was 412151 bytes; this patch adds 611 compressed bytes.
- `make browser-format browser-check`: pass after correcting new test assertions.
- `make architecture-check`: pass.
- `make test-wasm-focused AOE_WASM_TEST_PACKAGE=aoe-client AOE_WASM_TEST_FILTER=build_menu_snapshot`: pass.
- The same focused WASM target with `AOE_WASM_TEST_FILTER=actor_hit_projection`: pass.
- Extended focused e2e: 35 passed, zero skipped/flaky; after build-menu changes,
  16 placement/queue/HUD cases passed; after horizontal culling, 26 HUD,
  performance, selection-panel and minimap cases passed, zero skipped/flaky.
  After lazy picking, 22 cursor/garrison/combat/HUD cases pass as well.
  After resource/holdings work, 44 of 45 broader browser cases pass. The
  browser-defaults strategic return-to-near case fails on canvas opacity; the
  unchanged strategic-transition spec then passes all 10 cases alone. Both
  reports are retained; this does not establish that the first failure is fixed.
  Runs use `AOE_E2E_WORKERS=1`, ports 8196–8197 (earlier runs up to 8199).

The final focused browser commands (with the environment above) are:

```sh
AOE_E2E_WORKERS=1 AOE_E2E_PORT_RANGE=8196-8197 make test-e2e \
  AOE_E2E_SPECS='tests/village/hud/live-state.play.spec.ts tests/village/hud/dom.play.spec.ts tests/village/hud/layout.play.spec.ts tests/village/input/cursors.play.spec.ts tests/village/input/performance.play.spec.ts'
AOE_E2E_WORKERS=1 AOE_E2E_PORT_RANGE=8196-8197 make test-e2e \
  AOE_E2E_SPECS='tests/rendering/strategic-transition.play.spec.ts'
```

Measurements share a desktop with other work. An extra focused e2e runner was
signalled to stop during baseline startup, but its child briefly continued into
the normal phase before exiting. No affected samples are removed. Shared-host
load and short phases limit causal attribution and long-session claims.

Full suites, CI, publication and adversarial review are unrun. This session
stops under the four-hour rule; the performance acceptance remains unmet.
Synthetic test fixtures do not qualify finished RTS performance or dedicated hardware.

## Recording and cleanup

`SHIP_VIDEO=/home/anko/Work/projects/AoeWorld-cx-ui/.cache/showcase/ui-before-after.webm`:
31.72 seconds, 1920×1080, normal before/after followed by far before/after.
`make video-probe SHIP_VIDEO=...`: pass; all four extracted segment frames were
visually inspected with the original artwork and full HUD visible.
`make docs-check`: pass before commit; the commit hook checks the final document.

The final Playwright recorder reported paths without persisting files, including
on explicit save. The final after views instead use the screencast frame callback,
retain wall timestamps, and encode with Dockerized FFmpeg 5.1.9. The baseline
recordings are the retained original files. Intermediate timing journals and
recording metadata remain available, but some intermediate recording files are
missing. Video is separate content validation; timing was not rerun.

`DEV_PORT=8194 make -C /home/anko/Work/projects/AoeWorld-fps3-measure down`:
pass. The private server's original JavaScript and complete `web/pkg` tree are
restored and checksum-verified. The owner's port 8170 is untouched. The original
save and private copy still match their initial SHA-256.
