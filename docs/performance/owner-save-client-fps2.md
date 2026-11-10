# Owner-save client frame rate, round 2

Baseline code: `a78991dcc84aea1349dd8b8b476c42ca3d75fdab` (includes the
strategic painting/terrain-span review corrections). Method and save identity:
[round one](owner-save-client.md). The original save remains read-only.

## Method

Visible Chrome 156, 1280 × 720, hardware WebGL2:
`ANGLE (AMD, AMD Radeon Graphics (radeonsi renoir ACO), OpenGL ES 3.2)`.
Camera (1588, 612), normal zoom 1, far zoom 0.04. Both views run for
120 seconds per build. Readiness requires current-view painting, settled
terrain, the restored map identity and a subsequent native frame; the initial
base also requires a TownCenter HUD actor. Recording runs separately.

The server and 99 AI opponents keep advancing on a shared host. These are
informational runs, not fixed-tick comparisons or dedicated-hardware
qualification. Baseline human population/buildings: 2866/7628; sorting run:
2939/7750. The ending server telemetry reported 6.20 and 4.95 ticks/s,
respectively. This does not establish finished RTS performance.

## Batch scene ordering

Actors used entity-ID order for last-presented position lookup, but adding each
building, animal or rubble entry shifted the growing vector tail. The client
now appends them and performs one stable sort before projectiles and previews.
Equal-ID bridge pieces retain their span order. Wall indexing and sprite
selection are unchanged.

| Observation | Baseline | Batch ordering |
| --- | ---: | ---: |
| Normal FPS | 13.99 | 15.97 |
| Normal frame p50 / p95 / max, ms | 65.2 / 122.1 / 306.9 | 53.7 / 111.7 / 608.0 |
| Far FPS | 16.90 | 13.17 |
| Far frame p50 / p95 / max, ms | 41.7 / 144.8 / 1199.4 | 45.4 / 212.1 / 1531.1 |
| Normal interpolation stage p50 / p95, ms | 9.65 / 12.55 | 6.0 / 9.9 |
| Normal culling stage p50 / p95, ms | 7.9 / 10.15 | 5.6 / 7.4 |
| Normal sprites stage p50 / p95, ms | 7.35 / 11.5 | 7.9 / 12.2 |
| Normal UI stage p50 / p95, ms | 0.3 / 58.85 | 0.4 / 58.8 |

Stage rows are medians of five-second per-frame summaries, not whole-run
percentiles; spans overlap. Sorting is bypassed in strategic view. Its far run
was slower, and normal maximum frame time rose; the cause of those uncontrolled
differences is not established. Neither view meets the smooth-game goal yet.

The baseline ten-second CPU profile sampled 270 ms in scene-vector insertion;
that function is absent from the sorting run's thirty largest sampled functions.
Layout reads remain prominent: normal baseline `clientWidth` samples total
1260 ms, and the far profile totals 1524 ms. A separate thirty-frame observer
recorded 60 unchanged writes each to several strategic controls and to the
hidden reveal canvas's width and height. Those observations motivate the next
investigation; they do not assign a timing improvement to a future change.

## Evidence and checks at this increment

Local evidence: `.cache/owner-save/before.json`, `sort.json`, their normal/far
CPU profiles, and `sort-runtime.patch`. Served WASM response hashes match the
local artifacts:

- Baseline: `c8663cecaeaa5ab64739dc2ce52633efdbac4470e259eeb3459af20bbea920d4`.
- Sorting: `d8f7b05ca96d180b5d8463cdd5f7e8302ed292836c6e3519a7609b7ce1b0b42b`.

The sorting build was measured as the working production diff on the baseline;
the accompanying source commit preserves that change. Its gzip is 400,170 bytes
against the unchanged 375,952-byte baseline (+6.4%, within the unchanged +25%
cap). Diagnostics is 90,233 bytes.

Two new WASM regression cases cover 8194 shuffled scene entries, rubble and
interleaved bridge-piece order. They are compiled by lint. Standalone WASM
execution requires two simultaneous host ports; the brief permits only 8192.
Native client tests pass 31/31. The review corrections separately passed 8/8
focused browser cases after both new regressions failed on the previous code.
Sorting's performance/input and bridge-placement browser flows pass 24/24,
with no skipped, flaky or failed cases.

Commands use `NEXTEST_TEST_THREADS=2 CARGO_PROFILE_DEV_DEBUG=0`:

```text
make hooks-install hooks-check fmt-check lint structure-check architecture-check browser-check perf-wasm-cap
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-client AOE_NATIVE_TEST_FILTER='all()'
AOE_E2E_WORKERS=1 AOE_E2E_PORT_RANGE=8192-8192 make test-e2e AOE_E2E_SPECS=tests/rendering/strategic-transition.play.spec.ts
AOE_E2E_WORKERS=1 AOE_E2E_PORT_RANGE=8192-8192 make test-e2e AOE_E2E_SPECS='tests/village/input/performance.play.spec.ts tests/village/input/bridge.play.spec.ts'
```

The initial sorting browser closed before its sample completed; that attempt
is excluded and retained as `sort-interrupted.json`. The completed baseline
logged two aborted optional minimap requests; sorting logged no request errors.
The first recordings produced no usable files: the pinned recorder requested
VP9, but the host's bundled FFmpeg supports VP8 only. Recapture with a private
VP9-capable recording encoder and final qualification remain outstanding.
Original art and all recordings stay outside Git. No CI or publication has run
for these revisions.
