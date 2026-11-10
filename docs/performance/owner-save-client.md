# Owner-save client measurements, 2026-11-10

Telemetry: `bbf2409501016e69fe992c3005220cc194aa4284`.
HUD: `e9161a32717692a69f3a7f2028ccddf12a20b1b2`.
The wall candidate is that HUD revision plus the adjacent `scene.rs` wall-index
change committed with this report. Its exact working diff and WASM SHA-256
are recorded in `.cache/owner-save/final.json`; `wall-fix.patch` preserves the diff.

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

An additional current-map/TownCenter/later-native-frame observation was
5013 ms after the HUD fix and 3054 ms with the wall index.
It was observed concurrently with cadence sampling. No equivalent strict
baseline was captured. Cold-context opening remains expensive: the HUD video
needed about 21.2 seconds before its checked recording start.
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
GPU memory. The final cold-context recording start was 20.7 seconds. Fetch
records alone do not establish its parsing/decode/upload breakdown; further
cold-opening work and smooth-game qualification remain outstanding.
