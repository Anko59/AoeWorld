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
Uncovered time is reported as unattributed. The raw frame spans also break down
terrain samples, foundations, mesh construction and decoration. The sibling
Chrome traces retain the original browser events.

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
