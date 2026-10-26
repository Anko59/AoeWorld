# WebGPU instance submission

The comparison base is `c028a9d11ca5e6657c3c52bc8fb5ddfd30a60bad` on
`perf/webgpu-sprites`. The workload is a synthetic 16,384-square world,
16,384 admitted units and 82,944 resident trees, at 1280 by 720 with `?perf=1`.
This is a client/rendering workload, not finished RTS or hardware qualification.

## Findings and change

The base already retained its GPU instance buffer and atlas/sampler bindings.
It uploaded one packed instance list per submitted frame and batched contiguous
shader runs across atlas-array pages. It did not create buffers/bind groups per
sprite, repeatedly upload the atlas, use render-time `mapAsync`, or enable
WebGPU timestamp queries. The shared stable scene sort precedes both GPU paths.

WebGPU fetched its 112-byte instance packets through a vertex storage buffer.
It also submitted the entire world to the contour vertex shader, which rejected
terrain and ordinary trees on the GPU. WebGL2 used seven instanced vertex
attributes and submitted only eligible contour runs.

Both paths now consume the same seven attributes. Buffer growth no longer
requires a new bind group because bindings contain only the atlas and sampler.
WebGPU emits eligible, contiguous contour runs without repacking or reordering
instances. Its packet vector and draw-range vectors retain their allocations;
the game builder fills the retained packet directly. The depth view is cached
until resize. Instance capacity, the 64 MiB bound, shader arithmetic, atlas
pixels, alpha/depth ordering and existing snapshots stay unchanged.

Retained CPU packet/range capacity is included in `render_frame_bytes`.
The extra retained depth view is included in persistent GPU resources. Scene
layers, sorting and browser/wgpu submission objects still allocate; this is not
whole-frame allocation elimination.

## Measurement method

`browser/tests/rendering/performance/sprites.spec.ts` uses the existing
population, terrain, zoom and frame-sampling helpers. Each scene records thirty
samples after ten warmup animation frames, with the upper median and existing
p95 calculation. The regular village uses a fresh browser context and the live
server scene. WebGL2 runs in default Chromium with WebGPU disabled; WebGPU uses
the existing Vulkan SwiftShader launch flags. Timing cases disable trace and
screenshot recording. Existing rendering/snapshot cases keep their settings.

A stationary scene can be cached: animation-frame intervals alone do not prove
GPU presentation cadence. A separate cache invalidation records an actual
submitted frame's API calls. It uses the existing restoration event, which only
clears the last-frame cache; it does not recreate the device, context or atlas.
CPU API durations and submission intervals are not GPU timestamp measurements.

The base browser bundle was retained only as an ignored, temporary measurement
input; production has one current shader/buffer interface. Its WASM SHA-256 is
`9698eedd1bd0520033ea2dce04c91329402fc054f7d14e6d51ecec1937e6d8d0`.
The retained bundle is removed after comparison. Original art and derived images
remain outside Git and Docker images.

## Measurements (milliseconds)

Chromium `156.0.8078.4`, generated CI art, one local shared-host run per entry.
Rows give median / p95 frame intervals from the unchanged sampler. The WebGL2
control also improved between runs, so these numbers do not isolate a speedup.

| Scene | Base WebGPU | Current WebGPU | Base WebGL2 | Current WebGL2 |
| --- | ---: | ---: | ---: | ---: |
| Sprite LOD | 31.2 / 197.4 | 27.2 / 158.9 | 28.8 / 179.3 | 25.8 / 153.4 |
| Marker LOD | 16.7 / 17.6 | 16.7 / 16.8 | 16.7 / 16.9 | 16.7 / 16.8 |
| Map LOD | 16.7 / 17.1 | 16.7 / 16.8 | 16.7 / 16.8 | 16.7 / 16.8 |
| Regular village | 16.6 / 17.0 | 16.7 / 16.8 | 16.7 / 16.8 | 16.7 / 17.3 |

Sprite CPU draw medians were 28.5 -> 23.6 ms (WebGPU) and 24.7 -> 23.4 ms
(WebGL2). Cached sampling windows can contain no GPU submissions; these are not
rendered-frame throughput or GPU execution-time measurements. The original
population spec also passed unchanged at the base: WebGPU 21.9 / 150.9 ms,
WebGL2 22.3 / 152.0 ms, Canvas 21.7 / 147.2 ms. The supplied 214.9-versus-29.1 ms
median gap was **not reproduced**, and WebGPU-versus-WebGL2 hardware parity is
**not established**. The original report in the base worktree started at
02:58:29 UTC, before the 03:13:50 UTC base commit, and records median UI time of
174 ms and CPU draw time of 33.3 ms for WebGPU. It contains no GPU elapsed-time
measurement, so its frame-time ratio cannot isolate a GPU root cause.

The separately captured dense WebGPU frame had these actual API counts:

| Work | Base | Current |
| --- | ---: | ---: |
| Instance upload calls / bytes | 1 / 1,699,264 | 1 / 1,699,264 |
| World instances | 15,172 | 15,172 |
| Contour instances | 15,172 | 1 |
| Draws / pipeline sets | 2 / 2 | 2 / 2 |
| Bind-group sets / creations | 3 / 0 | 3 / 0 |
| Texture views created | 2 | 1 |
| Buffers created / texture uploads | 0 / 0 | 0 / 0 |
| Render-time mapAsync / query sets | 0 / 0 | 0 / 0 |

WebGL2 submitted three instanced draws: world, coverage, and one eligible
contour, totaling 30,345 instances, with one buffer upload. Neither backend
submitted one draw per sprite. Current WebGPU submits 15,173 total instances,
versus 30,344 at the base; output order and atlas pages are unchanged.

## GPU execution diagnostic

A separate temporary probe requested `timestamp-query` on the WebGPU device and
measured from the beginning of the world pass through the end of the contour
pass. It retained one four-slot query set and two 256-byte buffers, resolved
queries on submission, and asynchronously mapped the result outside production
drawing. Production still requests no timestamp feature or per-frame readback.
Three warmup submitted frames preceded thirty measured frames, with a cache
invalidation for each actual redraw. Browser presentation and CPU/UI work are
outside these pass timestamps. On the same shared-host SwiftShader setup:

| GPU pass elapsed, ms | Base | Current |
| --- | ---: | ---: |
| Median | 1,256.24 | 1,086.14 |
| p95 | 1,300.22 | 1,099.29 |

The observed median is 13.5% lower in this diagnostic, not a sevenfold gain.
An additional five-frame phase diagnostic measured median world/contour time
of 1,097.32 / 190.76 ms at the base and 1,071.51 / 0.08 ms currently. This small
sample identifies the avoided contour work; it is not the thirty-frame p95 run.
The WebGL2 asynchronous timer extension was unavailable in both runs, so no
cross-backend GPU-time or hardware-parity conclusion follows. Raw samples are
in `reports/gpuperf/timestamps.json` and `gl-timestamps.json`. These actual-redraw
GPU timings must not be compared directly with cached idle frame intervals.
The unit fixture spans the full world; this sprite camera has one eligible unit
and 15,172 total world instances, rather than 16,384 onscreen unit sprites.

Exploratory continuous cache invalidation was a different, much heavier stress
workload (base WebGPU about 1.28 s, WebGL2 about 1.62 s), not the reported case.
It is excluded from the headline comparison. Imported trial pack
`80d86805bb0ae23d78d26bc64904b068337323d9d18a3d2b1098ecc11dccdc60`
initialized, but the private-art matrix hit timeouts and WebGPU recovery;
those incomplete runs are not performance evidence. No existing runtime
watchdog, gate, baseline, size cap or snapshot changed. Firefox/GTX 1660 Ti and original-art
steady rendering still need qualification.

Raw local evidence is in ignored `reports/gpuperf/public-paired.json` and
`public-paired-playwright.json`; failed exploratory inputs remain separately
identified. The current measured browser WASM SHA-256 is
`eb4fa348defd3ed25c7a69ce8a0df56f2954f6085d1bfe10ed1c2d8498584ca1`.

## Reproduction and validation

Current measurement command (Dockerized, disposable server on an ephemeral port):

```sh
make test-e2e AOE_E2E_SPECS=tests/rendering/performance/sprites.spec.ts
```

The original population spec was reproduced without modifications using
`make test-e2e AOE_E2E_SPECS=tests/rendering/bigview-population.play.spec.ts`.
The structural regression command was `make test-wasm-focused
AOE_WASM_TEST_PACKAGE=aoe-rendering AOE_WASM_TEST_FILTER=frame_budget`.
`make perf-wasm-cap` passed at gameplay 288,227 B and diagnostics 90,232 B,
against the unchanged 302,087 B cap (baseline 287,702 B plus 5%).


The focused `frame_budget` browser tests first failed at the base: 65 contour
instances instead of two, and no instanced vertex layout. After implementation,
all four focused cases passed, including backpressure and cross-page contour
runs with negative/NaN surface tags. Existing tests and snapshots were not
rebaselined. The final Make commands, exit results and revision are retained
locally in `reports/gpuperf/checks.json`; CI and dedicated hardware were not run.
No full native or WASM suite was manually repeated. Browser probe lint/style
failures were corrected and the fast browser check rerun without rule changes.
The full native suite passed 3,337 tests (18 canonical skips), plus one doctest;
the full WASM suite passed 550 tests. Formatting, structure, Rust/browser lint,
WASM size, `perf-ci`, and the preflight components below passed.

The unchanged focused E2E set completed with 39 passes, three far-zoom grid
failures and six existing duplicate-project skips. All three grid failures
reproduced using the exact base browser bundle (SHA-256 above): WebGPU, WebGL2
and Canvas each produced zero darkened grid pixels after strategic zoom, where
`grid.play.spec.ts:105` requires more than 0.02. They remain an explicit existing
failure, outside this GPU submission patch; no test or grid behavior was changed.
The separate base check is retained in `reports/gpuperf/base-grid-e2e.json`.

Preflight is executed in components: `make preflight-checks`, the one required
full `make test-unit`, and `make perf-smoke`. The inherited Makefile difference
from `origin/dev` makes ordinary preflight select the full native workspace
again; component execution honors the owner's one-full-suite limit without
removing a check. Native commands use `CARGO_PROFILE_TEST_DEBUG=0` and
`CARGO_INCREMENTAL=0` for disk use, preserving assertions and test selection.
Firefox 157's Docker check reported no WebGPU adapter and passed via WebGL2
fallback; the physical GTX 1660 Ti/595.91.07 was identified but not timed.
