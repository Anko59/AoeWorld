# Owner-save client measurements, 2026-11-10

Telemetry: `bbf2409501016e69fe992c3005220cc194aa4284`.
HUD: `e9161a32717692a69f3a7f2028ccddf12a20b1b2`.
Wall index: `94d1f7e988f58239f31c4670242dbcc6686a50e2`.
The wall measurement used that commit's client change before it was committed.
Its exact working diff and WASM SHA-256 are recorded in
`.cache/owner-save/final.json`; `wall-fix.patch` preserves the matching diff.

The copied owner save is `1794176817-first.save`, SHA-256
`c51582bf7eb4007ac5cdb72ef2ab3a733c7a451532b6b41d6816a2fc9c85c11a`.
The source was not modified. Assets, recordings and detailed local evidence
remain outside Git.

## Workload and method

Visible headful Chrome 156.0.8078.47, 1280 x 720, WebGL2, actual renderer
`ANGLE (AMD, AMD Radeon Graphics (radeonsi renoir ACO), OpenGL ES 3.2)`.
No software renderer was used for these FPS observations. Camera (1588, 612),
normal zoom 1 and far zoom 0.04. Each view ran at least 120 seconds; recordings
were captured separately. The restored match has 99 AI opponents in eight hosts.

- before: 100 seats, human population 2870, 7708 human buildings
- after: 100 seats, human population 2930, 7719 human buildings
- final: 100 seats, human population 2940, 7890 human buildings

The match continues advancing and the shared host has competing workloads.
These are informational observations, not a fixed-tick comparison or dedicated
hardware qualification. Server verification before launch is excluded.

## Cadence and stage results

Frame triples are p50 / p95 / max in milliseconds, computed over each complete
rAF sample. Normal samples start at the terrain/frame readiness proxy and
therefore include any remaining loading stalls.

| Measurement | Telemetry baseline | HUD fix | HUD + wall index |
| --- | ---: | ---: | ---: |
| Normal FPS | 2.76 | 12.92 | 16.27 |
| Normal frame ms | 252.5 / 587.2 / 29280.4 | 69.3 / 122.5 / 883.3 | 53.3 / 109.5 / 334.4 |
| Far FPS | 18.79 | 24.87 | 21.71 |
| Far frame ms | 33.6 / 133.1 / 2491.9 | 29.8 / 112.0 / 1094.1 | 30.5 / 122.8 / 1324.3 |

The next table gives medians of five-second **per-frame** stage p50 / p95
summaries, not whole-run stage percentiles. HUD work runs every 250 ms; frames
without a HUD update contribute near-zero UI spans. Stage spans overlap and
must not be summed into frame time.

| Normal stage ms | Baseline | HUD fix | HUD + wall index |
| --- | ---: | ---: | ---: |
| update | 6.9 / 24.1 | 5.2 / 11.7 | 2.6 / 9.8 |
| interpolation | 25.5 / 45.6 | 24.8 / 27.4 | 8.9 / 13.3 |
| culling | 6.0 / 8.9 | 5.8 / 6.3 | 6.0 / 8.4 |
| terrain | 0.9 / 2.6 | 0.9 / 1.1 | 0.9 / 6.3 |
| sprites | 7.6 / 12.8 | 6.6 / 10.6 | 6.8 / 11.1 |
| ui | 174.2 / 295.6 | 0.3 / 55.2 | 0.3 / 56.0 |
| fog | 2.2 / 2.7 | 2.1 / 2.4 | 2.1 / 2.6 |

The HUD serialized 10,574 actors before the change and 86 after viewport
filtering in the sampled normal view. It now caches CSS scale and reuses
presented selected-unit poses. Offscreen selected actors remain available;
population, worker and idle totals still cover the entire known world.

A separate 20-second Chrome CPU profile after the HUD fix attributed 3,757.6 ms
of sampled self time to `building_ui::wall_frame`, its largest function.
Every wall checked up to eight neighbors by scanning the full wall vector.
The wall index keys tile coordinates and wall family in a hash set. The
silhouette function and its ordered eight-neighbor checks are unchanged;
no output depends on hash-set iteration. The remaining scene cost includes
actor preparation, native visibility and rendering; 60 FPS is not established.

## Opening and moving the normal view

The readiness proxy requires connected real assets, no pending/failed terrain
chunks and a rendered frame. Baseline video showed that this can precede the
current-map native view. These rows are **proxies**, not proven complete-load
times. The baseline helper cleared its resume token; later runs reused the
session to avoid a reload race, so reload times are not a controlled speedup
claim.

| Proxy ms | Baseline | HUD fix | HUD + wall index |
| --- | ---: | ---: | ---: |
| Reload proxy | 21850 | 3956 | 1771 |
| Normal jump +64,+32 | 2992 | 937 | 941 |
| Warm return | 1963 | 442 | 414 |
| Far transition | 6475 | 775 | 1347 |

The stronger observations below require the restored map identity, a TownCenter
actor and a later rendered native frame. The reload observation runs concurrently
with cadence sampling. The recording observation uses a fresh browser context,
then captures a screenshot before timing the recorded segment.

| Normal-view observation, ms | Baseline | HUD fix | HUD + wall index |
| --- | ---: | ---: | ---: |
| Reload: current map, TownCenter, later native frame | not captured | 5013 | 3054 |
| Fresh recording context: checked native scene | not captured | 21186 | 20718 |

These are distinct from the earlier readiness proxy. No equivalent strong
baseline was captured, and a fresh context does not establish cold OS/server
caches. Normal opening still takes about 21 seconds in the recording contexts.
Cold-cache completeness and arbitrary camera/device recovery remain unqualified.

The HUD-run normal jump fetched four terrain responses and six native frame
chunks. Warm return fetched nothing. Existing native streaming admits at most
16 cell uploads or 8 MiB per frame; these budgets were not changed. This work
reduces CPU work before those bounded updates, without shrinking native art.

## Verification and local artifacts

- Native client focused tests: 26/26; rendering: 5/5; server telemetry/admin: 12/12.
- Focused performance E2E: 14/14 after each fix, with no skips or flakies.
- Telemetry requests in the completed HUD run returned HTTP 204, and its
  `/admin/perf` snapshot contains the recent client report.
- The broader strategic spec remains 8/9: its General return-to-normal WebGPU
  path reloads after the five-second watchdog and loses the timing hook. This
  also reproduced before the HUD and wall changes; its cause is not established.
  No timeout, gate or baseline was weakened.
- WASM unit tests were compiled by lint/pre-commit, not executed: the configured
  runner requires two simultaneous host ports, outside the 8186-only constraint.
- Full suites, CI and dedicated-hardware qualification were not run. No ship
  or push was requested for this branch.

Commands (Dockerized Make; final work uses `CARGO_PROFILE_DEV_DEBUG=0`):

```text
make hooks-install hooks-check fmt-check lint structure-check architecture-check browser-check perf-wasm-cap docs-check
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-client AOE_NATIVE_TEST_FILTER='all()'
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-rendering AOE_NATIVE_TEST_FILTER='all()'
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server AOE_NATIVE_TEST_FILTER='test(client_perf)|test(admin::)'
AOE_E2E_WORKERS=1 AOE_E2E_PORT_RANGE=8186-8186 make test-e2e AOE_E2E_SPECS=tests/village/input/performance.play.spec.ts
AOE_E2E_WORKERS=1 AOE_E2E_PORT_RANGE=8186-8186 make test-e2e AOE_E2E_SPECS=tests/rendering/strategic.play.spec.ts
```

Structured samples: `.cache/owner-save/before.json`, `after.json`, `final.json`.
The baseline retains run/window summaries; the later files also retain raw rAF
intervals. CPU profile: `after.cpuprofile`, `profile-summary.json`. Failed
startup and memory-interrupted attempts are preserved separately and excluded.

VP9 WebM, with first/middle/final frames visually checked:

- `.cache/showcase/owner-save-before.webm`: 18 seconds, native base then far view.
- `.cache/showcase/owner-save-hud.webm`: 24 seconds, HUD fix.
- `.cache/showcase/owner-save-after.webm`: 24 seconds, HUD plus wall index.

Final `hooks-install`, `hooks-check`, `fmt-check`, `lint`, `structure-check`,
`architecture-check`, `browser-check`, `perf-wasm-cap` and `docs-check` passed.
Gameplay optimized WASM gzip is 396,326 bytes against the unchanged 375,952-byte
baseline (+5.4%, unchanged +25% gate); diagnostics is 90,233 bytes. Final E2E
report: `.cache/owner-save/final-e2e.json`; gate log: `final-gates.log`.
`SHIP_VIDEO=<absolute clip path> make video-probe` confirmed VP9, 1280 x 720,
25 FPS, 18 seconds before and 24 seconds after. Both probes are retained locally.

The final timed page's Resource Timing records include three manifest bodies
(39,908,589 bytes), 33 PNG body entries (62,334,244 bytes) and 30 native cell
chunks (30,656,983 bytes), including camera moves and repeated cached entries.
These body sizes follow HTTP content decoding; they are not decoded pixel or
GPU memory. The final fresh-context recording start was 20.7 seconds. Fetch
records alone do not establish its parsing/decode/upload breakdown; further
cold-opening work and smooth-game qualification remain outstanding.


## Startup source index (extension)

A cache-disabled fresh-context measurement without video capture took 21,238 ms
before source indexing and 12,641 ms after it to the checked normal scene.
The same map, camera, viewport and hardware WebGL2 path were used. This check
requires the current map, a TownCenter actor, settled terrain and a subsequent
native frame; screenshots show the complete sampled base. The live match and
shared-host load continue changing, so these are informational single runs.

| Observation, ms | Before index | After index |
| --- | ---: | ---: |
| Local art initialized | 18048 | 9826 |
| Connected | 18449 | 10192 |
| Checked current-map native scene | 21238 | 12641 |
| Jump +64,+32 | 1047 | 1715 |
| Warm return | 1165 | 1085 |
| Distant owned base (1123,895) | 845 | 778 |
| Return from distant base | 1041 | 1050 |

The startup CPU profile sampled about 4.2 seconds in source-frame searches,
building-part resolution and building selection. Repeated queries scanned the
whole 157,854-record catalogue, including unrelated native actor sheets.
The loader now builds a source/frame lookup once. It keeps original record
indices and validates each requested contiguous prefix, rejecting duplicates
and missing rows as before. Any mutable metadata access invalidates the index,
including overlay iteration, append and direct edits. The manifest format and
pixel/animation selection remain the current format; no alternate path is kept.

Five new native tests cover interleaved records, malformed prefixes, large
catalogues, overlay append and mutation invalidation. Client native tests pass
31/31. A second, profiled pair measured 20,317 ms before and 16,744 ms after;
source searches no longer appear among the twelve largest sampled functions.
The two pairs show the expected shared-host timing spread. Existing
source-selection and strict-manifest WASM tests are compiled by lint; standalone WASM execution retains the port limitation described above.

Evidence: `.cache/owner-save/loading-before.json`, `loading-index-verified.json`,
`loading-profile-before.cpuprofile` and `source-index.patch`. The two aborted
terrain requests in the after run were superseded camera demand; checked
frames report zero failed terrain chunks. The direct-capture setup helper was
corrected to perform the normal UI's existing reconnect action after restore;
disconnected attempts are excluded. Camera checks allow views without a
TownCenter; the initial busy-base check still requires one.

Startup and performance browser flows pass 17/17 across their configured
WebGPU/WebGL2 projects, with zero failures, skips or flaky results. Hooks,
formatting, lint, structure, architecture, browser and documentation checks pass.
WASM gzip is 398,815 bytes (+6.1% against the unchanged 375,952-byte baseline),
within the +25% cap. The normal base and four camera-jump screenshots were
inspected; no missing terrain or native art was observed in those sampled views.

## Current-view painting after load and camera changes

The cache-disabled before trace reported zero pending terrain at 18,727 ms,
while frame 2 still represented the earlier empty view; populated actors first
appeared with frame 3 at 19,532 ms. The indexed run also had a settled-terrain
interval before its populated native frame. A lifetime frame counter plus zero
pending terrain therefore did not establish that the requested view was drawn.

The client now publishes `data-view-painted` only when the current render is
presented (or its unchanged pixels remain valid), the accepted snapshot matches
the subscription revision, and the sent region matches the requested camera.
Waiting native coverage, backend presentation and a withheld snapshot leave it
false. World replacement clears it immediately, without resetting camera
preservation or the monotonic frame counter. The loading screen observes this
signal alongside its existing map, terrain, connection and new-frame checks.
This is a painting/snapshot condition; it is not a synchronization barrier for
every fog or resource-overlay update.

Tests were written first. Both new/resumed loading-screen cases failed on the
old condition because the screen vanished while the painting signal was false.
Browser coverage also withholds snapshots during opening, camera movement and
reconnect, then checks that painting becomes current after release. The unchanged
shot-clock helper moved into the existing lifecycle module to keep network.rs
below the 500-line limit.

Native client tests pass 31/31 and static/browser/WASM checks pass (399,082-byte
gzip, +6.2% against the unchanged baseline). The selected browser run passed
31/32; an existing menu-colour reload case timed out waiting for Single Player.
Its unchanged start-sequence file passed on the bounded fresh rerun. All new
readiness cases passed in the first run. The initial failure report is retained
as `.cache/owner-save/view-painted-first-e2e-report.json`; no test was relaxed.
