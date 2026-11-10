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
VP9, but the host's bundled FFmpeg supports VP8 only. A private VP9-capable
encoder recovered separate baseline, sorting and idle-UI recordings. Baseline
and sorting recapture identities match their preserved WASM and JS hashes.
Each exported clip is 24 seconds at 1280 × 720, with twelve seconds per view;
normal and far frames were inspected. Original art and recordings stay outside
Git. No CI or publication has run for these revisions.

## Idle strategic presentation

Code: `f0b4322dc` (on sorting commit `2c3b29631`). Unchanged normal frames
repeatedly hid strategic widgets and dismissed the reveal, including resetting
the hidden reveal canvas to 1 × 1. Control visibility now updates on transitions;
repeated hide calls retain their existing state. World reset invalidates the
control-state cache. Cinematic navigation still advances independently.

The new browser test failed twice on the old code with 160 mutations across
eight unchanged normal frames. It now observes zero mutations through two
near/far round trips. Transition and cinematic reveal cases pass 22/22, with
no skips or flakies. One gate attempt was refused below the unchanged 3 GiB
memory floor; the later completed run is retained separately.

| Observation | Sorting | + idle UI fix |
| --- | ---: | ---: |
| Normal FPS | 15.97 | 13.78 |
| Normal frame p50 / p95 / max, ms | 53.7 / 111.7 / 608.0 | 60.7 / 144.2 / 573.0 |
| Far FPS | 13.17 | 16.62 |
| Far frame p50 / p95 / max, ms | 45.4 / 212.1 / 1531.1 | 43.2 / 146.7 / 1191.6 |

Both samples cover 120 seconds per view. UI-run human population/buildings:
2905/7700; ending server rate 7.94 ticks/s. Its thirty-frame diagnostic contains
no redundant strategic/reveal visibility or canvas-size writes. Nevertheless,
the shared-host timings do not establish an FPS improvement from this change.
`dom.json` and its profiles retain the complete observation and served JS hashes.

## Viewport visual preparation

The next change bounds expensive pose, heading, building-composition and native
frame discovery by conservative art extents. Position interpolation and the
existing ground-height lookup still run before rejection. Envelopes include
every actor row and building state, damage/construction animations, signed
hotspots, part offsets, mirroring and alpha-shadow offset, at native world scale.
They retain a 512-pixel minimum for diagnostic markers and selection rings.
Uncertain/nonfinite bounds fail open. A prepared scene stores the bounds;
replacing colour art invalidates that scene. Its additional fixed envelope
storage is included in the existing scene-byte accounting.

The wall-neighbour index still covers all known walls. Authoritative unit and
building maps, offscreen selection data, global economy counts and native art
remain complete. Bridge spans retain their existing culling and stable piece
order; preview/projectile ordering is unchanged. The actor assembler moved to
its own module to keep source files below 500 lines.

Construction progress labels are also culled before serialization, using the
shared HUD gutter. The gutter preserves their 70-pixel bars and raised text at
viewport edges. Their private record count tracks the admitted labels; global
building totals and availability remain unchanged.

Tests were written first. The new native culling tests failed to compile before
their implementation existed, then passed. Current native client tests pass
36/36 and rendering tests 8/8, covering camera edges/zoom/elevation, signed
hotspots, offsets, mirrors, shadows and invalid metadata. An additional WASM
case checks that offscreen actors remain in authoritative and selected state.
WASM execution retains the two-port constraint described above.

The complete viewport source manifest is retained with the runtime patch in
`.cache/owner-save/culling-source-manifest.json`; all thirteen source hashes
were checked against the gated runtime. Served WASM:
`0d0129dde7c50cb0dc337d6182cac78c1e4a88183df0bd13e34cb318cc4530fb`.
Its gzip is 401,408 bytes, within the unchanged cap; diagnostics is 90,233 bytes.
Hooks, formatting, lint, structure, architecture and browser checks pass.
The original save and local copy still share SHA-256
`c51582bf7eb4007ac5cdb72ef2ab3a733c7a451532b6b41d6816a2fc9c85c11a`.

Additional viewport commands use the same thread/profile settings above:

```text
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-rendering AOE_NATIVE_TEST_FILTER='all()'
```

| Observation | Idle UI build | + viewport preparation |
| --- | ---: | ---: |
| Normal FPS | 13.78 | 33.33 |
| Normal frame p50 / p95 / max, ms | 60.7 / 144.2 / 573.0 | 24.7 / 75.5 / 172.5 |
| Far FPS | 16.62 | 20.67 |
| Far frame p50 / p95 / max, ms | 43.2 / 146.7 / 1191.6 | 32.6 / 126.1 / 1332.6 |
| Normal interpolation stage p50 / p95, ms | 6.35 / 10.9 | 3.1 / 6.4 |
| Normal culling stage p50 / p95, ms | 5.7 / 9.2 | 0.9 / 1.8 |
| Normal sprites stage p50 / p95, ms | 8.2 / 15.5 | 3.7 / 10.1 |
| Normal UI stage p50 / p95, ms | 0.3 / 67.15 | 0.2 / 50.0 |

These are two 120-second samples per build with the same camera and hardware
path. The normal sample exceeds the average-FPS target; its p95 still exceeds
the 33.3 ms budget. The far sample retains a 1.33-second stall, so smooth-game
qualification remains incomplete. Shared-host and advancing-match differences
prevent attributing every timing difference to this change. The far transition
took 881 ms; nearby camera jumps completed in 618 and 239 ms. No compilation
or video encoding ran during these cadence samples.

The viewport run began with human population/buildings 2866/7628 and ended
with server telemetry at 4.89 ticks/s. That server result is far below the
owner's separate clean-host 18.7 ticks/s observation and cannot qualify server
performance. One terrain-chunk request was aborted during the run; both sampled
views finished connected, painted and with no pending terrain. Complete raw
cadence, server data, camera timings and profiles are in `culling.json` and its
companion files. Remaining normal profile costs include layout reads and GPU
error checks; this increment makes no claim to resolve those costs.

## Video evidence

Final comparison: `.cache/showcase/owner-save-fps2-before-after.webm`.
VP9, 1280 × 720, 48.00 seconds: baseline normal/far occupies 0–24 seconds;
viewport-build normal/far occupies 24–48 seconds. First, middle and final
exported frames were inspected for native art, terrain and HUD completeness.
The individual 24-second clips are `owner-save-video-before.webm`,
`owner-save-video-sort.webm`, `owner-save-dom.webm` and
`owner-save-culling.webm` in the same ignored directory. Codec, dimensions and
duration are retained in `.cache/owner-save/*video-probe.json`. Recordings run
separately from cadence sampling and are not frame-rate proof.

Final touched-flow browser command (same thread/profile settings):

```text
AOE_E2E_WORKERS=1 AOE_E2E_PORT_RANGE=8192-8192 make test-e2e AOE_E2E_SPECS='tests/village/input/performance.play.spec.ts tests/village/input/bridge.play.spec.ts tests/start/sequence.play.spec.ts'
make video-probe SHIP_VIDEO=.cache/showcase/owner-save-fps2-before-after.webm
```

The first final browser run completed in 566 seconds: 28 expected passes,
four unexpected timeouts, zero skipped and zero flaky. All sixteen
performance/input cases passed across Firefox WebGL2 and Chromium WebGPU;
all four Firefox bridge cases passed. Chromium WebGPU timed out in three bridge
cases and the preset/loading sequence. The unchanged per-test budget is
30 seconds. The full report is preserved as
`.cache/owner-save/culling-e2e-first.json`, including error diagnostics. These failures
are not explained by the native/static passes, and no root cause is established.

One unchanged-source retry of the bridge and startup files completed in
385 seconds: 12 passes, four failures, zero skipped and zero flaky. Firefox's
four bridge cases passed again. Chromium failures were a bridge mouse-move
30-second timeout, another bridge fixture's population remaining 0/0 within its
10-second expectation, the direct-game canvas remaining hidden within ten
seconds, and no held terrain request arriving within the loading test's budget.
The retry report is `.cache/owner-save/culling-e2e-retry.json`; its retained
failure traces are under `.cache/playwright-results`. This is not a clean
browser validation. The cause and whether it predates the viewport candidate
remain unverified; no test budget or assertion was changed. The viewport
candidate is preserved for review, not qualified for publication.

`make video-probe` passed for the final 48-second comparison. The goal's
average normal-FPS target was observed, but far-view smoothness and the browser
failures remain outstanding at the end of the time box. Standalone WASM tests,
full CI suites and dedicated-hardware qualification were not run.

## Final in-process confirmation

After the owner finished playing, the unchanged production code at
`fa7b6f8d87952aa8b567dfdf3ed6cb38d6b03ef4` was measured again in visible
Chrome 156.0.8078.47 on the same hardware WebGL2 path, camera and viewport.
This confirmation waits for the restored map hash, TownCenter, settled terrain
and a subsequent painted native frame before starting each 120-second sample.
The earlier runs started 0.2–0.8 seconds before that strict readiness point.
RAF, game-stage summaries and Long Tasks were collected inside the page;
no CDP profiler, compilation or video encoding ran during cadence sampling.

| Code / sample | Normal FPS | Normal p95, ms | Far FPS | Far p95, ms |
| --- | ---: | ---: | ---: | ---: |
| `a78991dcc`, baseline + review fixes | 13.99 | 122.1 | 16.90 | 144.8 |
| `2c3b29631`, batch ordering | 15.97 | 111.7 | 13.17 | 212.1 |
| `f0b4322dc`, idle visibility | 13.78 | 144.2 | 16.62 | 146.7 |
| `fa7b6f8d8`, viewport preparation | 33.33 | 75.5 | 20.67 | 126.1 |
| `fa7b6f8d8`, final in-process confirmation | 43.30 | 63.2 | 31.98 | 97.7 |

Final normal frame p50/max: 17.4/477.1 ms; far: 18.4/1228.3 ms. Average FPS
exceeds 30 in both final samples, but those tails do not establish smooth play.
The observer recorded 421 normal and 475 far long tasks, with longest durations
469 and 1064 ms. These are observed main-thread task durations, not isolated
CPU costs or proof of a particular cause. Final normal-stage p50/p95 summaries:
interpolation 3.3/6.9, culling 0.9/1.9, sprites 4.0/8.4, UI 0.2/44.8 ms.
Far UI remains 0.1/78.9 ms by the same stage-summary method.

The shared host's one-minute load rose from 19.81 at capture start to 46.87
after normal and 37.46 after far, on twelve cores. These runs are not controlled
speedup estimates. Human starting population/buildings were 2866/7628; ending
server rate was 5.93 ticks/s. The source save and local copy retain the SHA-256
recorded above. Both cadence endpoints were connected, painted, with zero
pending terrain. Seventeen aborted telemetry/terrain/minimap requests are
retained in the raw capture; they are not silently excluded. Complete evidence:
`.cache/owner-save/final-inprocess.json`, its capture helper and video probe.
The served WASM hash still matches the viewport build listed above.

The final five-file browser run passed 53/54 cases in 322 seconds. The only
failure was an intro/menu case with 37 `ERR_NETWORK_CHANGED` console entries
while loading modules, before entering the game. One unchanged startup-file
retry passed 6/8 in 139 seconds: the original failing case passed, while two
other startup cases failed with nine and eight network-change entries. Those
two had passed in the first run. Every selected case passed at least once;
**neither complete command was green**. All strategic, reveal, performance/input,
bridge and preset/colour-loading cases passed. Assertions, retries and budgets
were unchanged. The broader startup/network instability is recorded separately
in [#757](https://github.com/Anko59/AoeWorld/issues/757); its cause remains
unestablished. Both reports and failure traces remain in `.cache/owner-save/`.

Final comparison video:
`.cache/showcase/owner-save-fps2-final-before-after.webm` — VP9, 1280 × 720,
48.00 seconds. Baseline normal/far occupies 0–24 seconds; the final confirmation
normal/far occupies 24–48 seconds. First, middle and final exported frames were
inspected for native art, terrain and HUD completeness. Recording was separate
from cadence sampling. The earlier comparison and per-step clips are retained.

Final commands retain `NEXTEST_TEST_THREADS=2 CARGO_PROFILE_DEV_DEBUG=0` and
the unchanged four-job container cap:

```text
make hooks-install hooks-check fmt-check lint structure-check architecture-check browser-check docs-check perf-wasm-cap
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-client AOE_NATIVE_TEST_FILTER='all()'
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-rendering AOE_NATIVE_TEST_FILTER='all()'
AOE_E2E_WORKERS=1 AOE_E2E_PORT_RANGE=8192-8192 make test-e2e AOE_E2E_SPECS='tests/rendering/strategic-transition.play.spec.ts tests/rendering/cockpit-reveal.play.spec.ts tests/village/input/performance.play.spec.ts tests/village/input/bridge.play.spec.ts tests/start/sequence.play.spec.ts'
AOE_E2E_WORKERS=1 AOE_E2E_PORT_RANGE=8192-8192 make test-e2e AOE_E2E_SPECS=tests/start/sequence.play.spec.ts
make video-probe SHIP_VIDEO=.cache/showcase/owner-save-fps2-final-before-after.webm
```

Native/static/size results are retained in `final-static-gates.log` and
`final-rendering-native.log`; browser results are `final-five-flows-e2e.json`
and `final-startup-retry.json`. Earlier failures remain evidence, even when a
later case passes. Full CI, standalone WASM execution under the two-port runner,
dedicated-hardware qualification and smooth-game qualification remain outside
the demonstrated results. No publication was performed.
