# Saved-game client stalls

The saved-game mode of `make client-frame-bench` measures the time between
successive ordinary client animation callbacks. It retains all intervals,
including callbacks that reuse previous pixels while chunks or art arrive.
The client counts successful scene submissions separately in its paint counter.

Copy the owner's France package and saves into an ignored local directory.
Put the package manifest at the root with its existing `pages/` and
`navigation/` trees. Put the matching `.save` and `.json` pair under `saves/`.
Use the save filename stem for `SAVE_ID` and its map hash for `MAP_HASH`.
Supply the base art pack from `local-assets/packs` with its relative
`buildings` and `actors` links. Keep those inputs and the output directory
outside Git and Docker images.

```sh
make client-frame-bench CLIENT_FRAME_BENCH_ARGS='--package-directory .cache/client-stutter/map-packages --content-hash MAP_HASH --asset-pack local-assets/packs/BASE_PACK --save-id SAVE_ID --out .cache/client-stutter/report.json'
```

Use the same save, map, art, viewport, browser and per-scene duration for
before and after. The default is 45 seconds per scene. Override the duration
with `--scene-seconds` (10 to 120 seconds) when bringing up a new save.
The existing integer-counter benchmark still runs when `--save-id` is absent.

The disposable loopback server restores the match through skirmish, with the
tick clock stopped at the saved tick. Fog is disabled for the capture so both
sides of the saved fight are visible. Camera subscriptions, snapshot decoding,
map requests, art streaming and animation still take their normal paths.
After warmup, the camera traverses the map between fixed fractional endpoints
and holds over the densest town with attackers. The driver verifies the hold
contains at least a thousand living units plus buildings and attackers in the
last submitted scene. The report also records the backing resolution.

The report includes nearest-rank p50 and p99, the maximum, and strict counts
above 50 ms and 100 ms normalized by elapsed minutes. Each long frame keeps the timed stages
that overlap it. Nested spans are assigned once, with specific work such as
WebGL uploads and main-thread garbage collection taking priority over their
parent phase. The category with the most exclusive time supplies the reported
cause.
The raw frame spans also break down terrain preparation, subscription sends,
wire decoding, snapshot application, chunk decoding and installation, and
native art streaming. The sibling Chrome traces retain JavaScript and WASM
callback envelopes, renderer tasks and compositor events. Awaiting animation
dispatch is measured between a traced request and its matching callback.
Recorded main-thread work takes priority over that interval, including
synchronous GPU command waits and pixel readbacks. Use the neighboring GPU and
scheduler trace events to investigate delayed dispatch. Uncovered time stays
unattributed. Each stall reports the fraction covered by the observed spans.
The frame records also identify when the detailed renderer is waiting for a
GPU queue slot, a visible terrain mesh or native art. These states explain
callbacks that do not submit pixels independently of their timing spans.

Keep the per-frame JSON, Chrome traces, browser recording and terminal log.
The report includes the input and WASM digests, browser version and renderer.
The measurements include recorder and instrumentation overhead. They use
SwiftShader on a shared host. Check the reported host load and submission count
when comparing runs.

## Build comparisons

The saved-game mode records the benchmark checkout revision and the client
artifact's checkout revision separately. To use the same disposable server
with two builds, build each instrumented client using
`make client-frame-bench-client-build CLIENT_FRAME_SOURCE=/path/in/this/checkout`.
Pass its `.cache/client-frame-bench/pkg` directory as
`--client-package-directory`. Use a checkout that contains the same
measurement hooks. Keep the exact source patch beside a dirty build.

Use the device, input digests, painted counts and renderer stage spans
alongside the timing table. Since the rate divides by elapsed time, a build
that completes more callbacks can have more long frames per minute even with
a shorter p99. The raw frames expose callbacks that did not paint and spans
that may need further investigation.

## France 11 comparison

The 45-second runs restored France 11 at tick 297818 with 63,379 units and
32,658 buildings. Both crowded holds started with 4,448 living units, 1,949
building sprites and 15 attackers in the submitted scene. The pan swept
between (300, 500) and (1700, 1500) at zoom 0.102. Both clients used the same
separate France terrain crash fix during the comparison.

| Scene | Build | P50 ms | P99 ms | Max ms | >50 ms/min | >100 ms/min | Callbacks | Submissions |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Fast pan | Before | 259.7 | 1486.9 | 1745.3 | 154.2 | 152.9 | 120 | 20 |
| Fast pan | After | 86.9 | 793.0 | 1860.2 | 221.2 | 203.9 | 307 | 21 |
| Crowded hold | Before | 278.3 | 10144.6 | 10144.6 | 80.0 | 80.0 | 61 | 8 |
| Crowded hold | After | 16.7 | 127.2 | 776.5 | 258.6 | 49.3 | 1965 | 13 |

WebGL validation in long crowded-hold callbacks fell from 13.76 seconds to
0.03 seconds. Measured terrain work in long pan callbacks fell from 24.63 to
19.48 seconds. The after trace includes terrain work requested by the HUD.
About 1.84 seconds of the worst 1.86-second after pan interval fell outside
instrumented stages. One-minute host load at the start of the pan
was 24.18 before and 19.64 after. For the hold it was 17.51 before and 12.67 after.

## Fast-pan attribution

The instrumented round-one client reproduced a 793.4 ms pan p99 and a
1,776.3 ms maximum. It submitted 19 scenes across 248 animation callbacks.
Observed trace or client spans cover every interval above 100 ms in this
capture. The longest interval includes 1,698.3 ms with an outstanding
animation-frame request and no recorded renderer main-thread task. The trace
shows 1.4 s of overlapping GPU context work followed by 586.5 ms on the GPU
command thread. Those spans locate the wait and the concurrent GPU work.
The next-longest interval includes 1,130.2 ms waiting synchronously for GPU
commands and 249.9 ms preparing terrain.

Across pan intervals over 50 ms, 21.64 s fell in terrain preparation
and 10.51 s awaiting animation dispatch. Wire decoding took 0.79 s, snapshot
application 0.15 s, texture uploads 0.025 s and buffer uploads 0.004 s.
A separate crowded-hold stall spent 1.20 of 1.23 seconds in synchronous
browser pixel readback as the fine minimap arrived. The trace retains main
and GPU thread wall times and CPU times where Chromium supplies them.

The live client now reuses world-space chunk meshes while the camera pans.
The prep queue serves the visible view and then a position projected along
the camera motion. Each turn limits the time spent preparing chunks and keeps
ready terrain while edge data or foundation changes are rebuilt. A shared
halo keeps heights and material transitions continuous across chunk edges.
New patches first supply native ground and edge geometry. Relief and
material blending refine those same surfaces after the visible and predicted
views have their meshes.
The subscription covers a margin around the current view and sends a
replacement from a separate browser task when that margin is crossed.

## France 11 round two

The matched 45-second captures use the same locally restored France 11 save,
map, art, 1280 by 720 backing and Chromium 159 with ANGLE SwiftShader. The
scenes ran at tick 297818 with 63,379 units and 32,658 buildings in the save.
Both captures used the same separate France terrain crash fix.

| Scene | Build | P50 ms | P99 ms | Max ms | >50 ms/min | >100 ms/min | Callbacks | Submissions |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Fast pan | Before | 124.6 | 927.6 | 1927.4 | 209.8 | 203.2 | 292 | 18 |
| Fast pan | After | 36.5 | 1385.9 | 1719.7 | 231.0 | 120.8 | 442 | 13 |
| Crowded hold | Before | 16.7 | 201.3 | 378.6 | 251.9 | 84.0 | 1758 | 13 |
| Crowded hold | After | 16.7 | 101.9 | 180.4 | 263.9 | 29.3 | 2089 | 16 |

World-space terrain patches start ahead of the moving camera and spread their
construction over animation callbacks. The base mesh keeps source shorelines
while material and relief refinement catches up. View changes schedule their
subscription sends in a separate browser task. WebGL2 reuses instance buffers
and waits for outstanding draws before allocating new storage.

Fast-pan terrain work in intervals over 50 ms fell from 19.56 to 4.84 seconds.
Waiting for animation dispatch rose from 13.24 to 24.64 seconds. The observed
spans covered all 153 baseline and 91 final pan intervals above 100 ms. The
longest final interval included 1,665.7 ms waiting for dispatch and overlapped
a 1,498.6 ms Chromium GPU raster task. The next-longest interval overlapped
1,719.4 ms in GPU context switching.

Both fast-pan targets remain unmet. One-minute load at pan start was 18.72
before and 14.46 after. Hold load was 14.55 before and 8.80 after. Chromium
reported a network change during the final pan that interrupted 20 chunk
requests. The callback and submission counts reflect the recorded run.
