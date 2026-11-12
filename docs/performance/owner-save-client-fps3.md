# Owner-save client frame rate, round 3

The viewport CSS cache from `88fbcd41bc6c693e139a5ff208a0dc11393bb9eb` is
removed. The replacement borrows presented geometry for picking and avoids
invalidating visible terrain when only offscreen foundations change. The final
campaign lowers frame p95 in both measured views on Firefox WebGPU and Chromium
WebGL2. WebGL2 normal median increases; this is not a uniform frame-rate win.

## Changes and tests first

Pointer picking borrows actors/resources from the last successful frame when
its scene identity still matches. A new camera/terrain scene prepares fresh
geometry. Live depletion, fog and starting-resource membership override old
pixels. Reset releases references; art invalidation retains the displayed
geometry until replacement succeeds. No second persistent scene cache is added.

Previously every building snapshot compared foundations across the whole world.
Remote construction could discard the visible terrain mesh. Foundation keys
now cover the actual terrain samples plus the largest native footprint and a
one-tile apron. Empty terrain remains conservative. Painting still receives all
buildings. Offscreen additions/age changes retain the mesh; entering the view
and overlapping native footprints invalidate it. A 16-tile footprint test
covers native metadata larger than the four-tile fallback.

Both changes have regression tests demonstrated red before implementation.
The corrected resource-body point is asserted outside the ground tile; the old
picker exposes that body before its pixels exist. The terrain regression shows
an offscreen foundation unnecessarily discarding the visible mesh. Tests stay
in canonical test files. The PR contains ten client source/test paths and this
report, with no owner dependencies, renderer changes or assets.

## Revision and reproduction

PR base: `141eb393048e6c4c84443ee67709b0960511cbd5`.
Measurement baseline: full owner commit
`d0a1677ecb63d56a2a14d52e751d5ebc07e0a162` (dev + #773 timers + #777 walls +
Mekong). The scratch index and initial source matched that commit exactly.
Only the client patch was applied for after builds; server, simulation, map,
renderer and assets remained on the owner revision.

Scratch: `/home/anko/Work/projects/AoeWorld-fps3-measure`, port **8194**.
Functional checks: **8196–8197**. Port 8170 was never used.
Save: `1794451672-mekong.save`, SHA-256
`7ae8dcf6de393245ca1a9dcb86f60242fa909a41c1d06f5c2a6aea69c3b5f839`.
It restores tick 184362, 11756 units and 7632 buildings, with 99 AI opponents
in eight hosts. All entities restore; **52 jobs are not resumed**. This is a
limit on reproducing the owner's exact uninterrupted session.

Map: `2976b04b4d746a9bc8196194fb3a31a87ddfb1fd5878788edc4aa621a0d5c401`.
Original pack: `8f7abe8c330d7fd710daa076c8ef07de65f8973feed59b99f4d01f9cbc9c4530`.
Original assets, save, profiles, journals, images and recordings stay ignored.

Visible browsers use 1920×1080 CSS pixels, DPR 1, capital (925,1277), zoom
**0.25 normal / 0.16 far**, both detailed sprite views. Firefox 157 uses
WebGPU; Chromium 156 uses WebGL2 for the final secondary check. Normal has
about 2063 WebGPU draws; far about 2628 and 830 HUD actors. The earlier sparse
zoom 1 / strategic zoom 0.04 workload's 25 ms medians do not qualify this task.

Long Firefox phases run 21 wall-clock minutes per view. Normal pans among
(925,1277), (957,1293), (893,1261), returning every 30 seconds, with selection
every 120 seconds. Far holds the capital with an owned actor selected. Fresh
save restores precede successful phases. Simulation time progresses more slowly
than wall time under this load. The first baseline normal completed; its next
far phase failed to settle a pan after seven minutes. That failed attempt is
retained. A fresh, fixed-camera far baseline completed 21 minutes.

Raw frame intervals retain every stall, excluding only the initial zero.
Stage numbers below are medians of five-second windows' p95, not additive
parts of the frame percentile. UI runs at four Hz. Memory sampling is about
every 30 seconds. Weak-reference listener estimates cannot observe automatic
once-removal and are not a heap census. Chromium also records CDP counts.
Supplemental OS sampling traverses descendants of every browser thread;
parent-only Firefox readings do not represent the whole browser.

## Long-run cost and growth

| Owner Firefox WebGPU | Duration | Frames | Frame p50 / p95 ms |
| --- | --- | --- | --- |
| Normal | 1269.5 s | 9250 | 128 / 221 |
| Far | 1269.0 s | 8674 | 137 / 250 |

The dense workload reproduces the reported magnitude, but **does not reproduce
hours-long deterioration**. No sustained UI/terrain/sprite growth is established.
UI is the largest steady stage; terrain tails decline as the scene settles.

| Far minutes | Frame p50 / p95 | UI | Terrain | Sprites |
| --- | --- | --- | --- | --- |
| 0–5 | 137 / 279 | 89 | 151 | 14 |
| 5–10 | 150 / 267 | 97 | 142.5 | 15 |
| 10–15 | 140 / 248 | 91 | 20 | 14 |
| 15–21 | 124 / 206 | 90 | 19 | 14 |

Far DOM stays around 2850 live elements, listener registrations at 386, and
terrain residency at 52 chunks. WASM grows 127139840→130940928 bytes then
plateaus. Resource entries grow 7778→9367 with authoritative world state;
this alone is not evidence of a leak. Whole-browser proportional memory cycles
around 1.3–1.5 GiB rather than growing monotonically. Roughly 300 child nodes
are replaced per second despite stable live DOM: costly churn remains.

A separate 45-second CPU profile attributes about 29% of HUD samples to
resource picking/preparation; actor picking also repeatedly rebuilds geometry.
That diagnostic guided picking reuse. Mean normal interval time outside the
union of stage spans is about 18 ms, including browser work and waits; it is
not assigned to GPU execution. Stage percentiles cannot attribute a remainder
by subtraction.

## One campaign per change, including failures

The old viewport cache is dropped: its full-owner sparse-view WebGL2 p95 was
41.1→63.2 ms normal and 114.9→122.1 ms far. Historical evidence is preserved
in `.cache/fps4/prior-report.md`.

Picking reuse alone ran 21 minutes in each view. Normal frame p50/p95 was
122/209 ms, UI 48; far 155/305 ms, UI 70.5, versus owner UI 53/90. Normal had
a brief marker excursion around minute 9.5; its ten retained buttons explain a
one-time +30 listener step, then flat registration with no detached targets.
All frames remain included. Far stayed in sprites. Unrelated host load rose
from a baseline sample near 12 to 24–30 on 12 logical CPUs. This limits causal
attribution; it does not erase the result. Picking-only Chromium WebGL2 normal
p95 was 395.5 ms versus 297.7 ms owner, so that candidate was not committed.
Its Chromium WebGPU capture is also retained, without a final qualification claim.

The foundation invalidation regression was then reproduced in a failing test,
fixed, and measured once as a second actual code change. No unchanged-code
timing retry was performed. Frozen plan: `.cache/fps5/plan.json`.

Final Firefox comparisons use the **first 180 seconds** of each owner long
phase and final short phase, including stalls, to match cold age and workload.
Full final phases are 187.9/185.3 seconds (1530/1390 intervals); no hidden
windows or marker LOD occur in the compared prefix.

| Renderer / view | Owner p50 / p95 ms | Final p50 / p95 ms | UI before→after | Terrain before→after |
| --- | --- | --- | --- | --- |
| Firefox WebGPU normal | 138 / 258 | 115 / 184 | 62→49 | 139→29 |
| Firefox WebGPU far | 135 / 273 | 125 / 198 | 85→62 | 143→19 |
| Chromium WebGL2 normal | 171.4 / 297.7 | 182.9 / 275.6 | 96→73 | 113.2→28.5 |
| Chromium WebGL2 far | 251.9 / 469.8 | 203.2 / 310.3 | 117.9→77.4 | 192.1→18.7 |

WebGL2 uses complete target-180-second phases: final normal 189.2 seconds /
1010 intervals, far 182.1 / 890. Its normal **median regresses 6.7%** while
p95 improves 7.4%; far p95 improves 34.0%. Final WebGPU p95 improves 28.7% /
27.5%. Draw counts remain comparable: WebGPU 2063→2063 / 2623→2628;
WebGL2 4489→4494 / 5111→5111. Native artwork is unchanged. The terrain and UI
cost reductions support keeping this replacement, with the shared-host limits
above. The final combined patch has not had another 21-minute ageing run.

Before WASM SHA-256:
`6e31dcccdc13676fde2493f558505623771b3271c93ffdbc59cdd815512c1446`.
Picking-only WASM:
`d599ce42d46ca151d5a3c33efa5f391b5a24cc11bd2749dbbccb2c8ab5056542`.
Final served WASM:
`fa4ba15cb2e1db8d5e0d79b138753d8eb92c9d4cb3ccab2108fa56071bfec4f4`.
Final client patch SHA-256:
`90de21d3c4f64cf48570461502e46b39f393bad4cf3e94819739036c80a53288`.

## Commands, checks and evidence

All compilation/checks use Dockerized Make, `CARGO_PROFILE_DEV_DEBUG=0`, and
`NEXTEST_TEST_THREADS=2` for focused native tests.

- `make -C /home/anko/Work/projects/AoeWorld-fps3-measure build-wasm`: baseline and both candidate builds pass.
- Private `make dev-start` with `DEV_PORT=8194` and the pack/map above: pass.
- `make test-wasm-focused AOE_WASM_TEST_PACKAGE=aoe-client AOE_WASM_TEST_FILTER=presented_picking`: expected red on the old picker. Corrected body regression: `.cache/fps4/picking-body-red.log`.
- Offscreen terrain invalidation expected red: `.cache/fps4/terrain-red.log`.
- `make hooks-install hooks-check fmt-check lint structure-check architecture-check browser-check`: pass. Initial index/test-location failures were corrected without weakening a gate; the cache test lives in `render_cache/tests/`.
- Final `make hooks-check fmt-check lint structure-check architecture-check test-native-focused perf-wasm-cap test-wasm-focused AOE_NATIVE_TEST_PACKAGE=aoe-client AOE_NATIVE_TEST_FILTER='all()' AOE_WASM_TEST_PACKAGE=aoe-client AOE_WASM_TEST_FILTER=playground::game::map AOE_E2E_PORT_RANGE=8196-8197`: pass; **37 native / 135 WASM tests**, zero failed/ignored.
- `make perf-wasm-cap`: client gzip **409278 bytes**, unchanged baseline 375952 (+8.9%), unchanged +25% ceiling; pass.
- `AOE_E2E_WORKERS=1 AOE_E2E_PORT_RANGE=8196-8197 make test-e2e AOE_E2E_SPECS='tests/village/input/performance.play.spec.ts tests/village/input/selection-panel.play.spec.ts tests/village/selection-density.play.spec.ts tests/village/production/selection.play.spec.ts tests/village/interface.play.spec.ts'`: **26 passed**, zero skipped/flaky/unexpected. Run after picking reuse, before the terrain-only change; includes Canvas replacement and player interaction.
- `make docs-check describe-check SHIP_TITLE='client: reuse picking geometry and retain unaffected terrain' SHIP_BODY=.cache/fps3-body.md`: pass after correcting required body headings and section length.
- `make video-probe SHIP_VIDEO=/home/anko/Work/projects/AoeWorld-cx-fps3/.cache/showcase/fps3-before-after.webm`: pass, 28.92 s at 1920×1080; separate visual evidence, never overlapping timing.

Raw long-run journals, profiles, host sampling and rejected-candidate logs:
`.cache/fps4/`. Final plan, journals, summaries, matched-prefix calculation,
build log and recordings: `.cache/fps5/`. Helpers: `node .cache/fps3/restore.mjs`,
`node .cache/fps5/run.mjs`, `python3 .cache/fps5/prefix.py` and
`python3 .cache/fps5/summarize.py <capture>`. The 28.92-second showcase contains labelled owner-baseline Firefox reference
stills and final Chromium WebGL2 motion, normal then far. The original recorder
returned paths without usable media; its VP9 encoder mismatch and failed
Firefox replacement are retained. This is visual art evidence, not a cadence
comparison; the recorder adds padding and clips the lower HUD. Native world
art was visually checked in both final views. Original media stays outside Git and is attached via SHIP_VIDEO.

Full suites, CI, adversarial review and publication are unrun. Final Chromium
WebGPU is unmeasured. Adapter identity is partly privacy-redacted. These
shared-desktop observations do not qualify dedicated hardware, finished RTS
performance, an hours-long leak fix, or the exact owner-session regression.
