# Deterministic client frame work

`make client-frame-bench` runs the client's own WASM animation callback against
a frozen, current-recipe map package. It reports integer work per frame, with
terrain, sprite, WebGL submission and upload-pressure rows. Descheduling does
not add work. No wall-clock timing or statistical CPU sampling contributes to
the score. Keep the vector: bytes, triangles and API calls have different units
and must not be added into an invented instruction or millisecond estimate.

## Run

Original assets, generated packages, reports, screenshots and videos stay outside
Git and images. The package directory must contain the current manifests and
their environmental pages. Absolute asset symlinks must become relative before
Docker can resolve them. This measurement used the current Mekong recipe 4,
package `4d9cb16c1c286a5b29ba1d94e70371af966ac7a130a1c3c0f6bcbe95f3b5b683`.

```sh
make client-frame-bench CLIENT_FRAME_BENCH_ARGS='--package-directory local-assets/map-packages --content-hash 4d9cb16c1c286a5b29ba1d94e70371af966ac7a130a1c3c0f6bcbe95f3b5b683 --asset-pack local-assets/packs/8f7abe8c330d7fd710daa076c8ef07de65f8973feed59b99f4d01f9cbc9c4530 --compile-load --out .cache/client-frame-bench/dev.json'
```

The target builds and binds a private `frame-bench` client, builds the server,
starts a disposable server on an unused localhost port and runs pinned Chromium
in Docker with WebGL2/SwiftShader. The normal gameplay bundle is not replaced.
The optional `--compile-load` adds three trials during an owned Dockerized Make
compilation after three baseline trials; without it there are three trials.
The compiler builds the server and harness with release fat LTO and one codegen
unit. Each invocation owns a fresh temporary Cargo target and an unused unique
cfg argument, preventing an object-cache hit from replacing the compiler load.
Its log must show compilation starting, and its Make process must remain alive
before and after every loaded trial. The owned process group is terminated at
the end, then its owned target is removed. Other work on this shared host is
not controlled; baseline means no
additional benchmark-owned compiler, not a qualified quiet machine.

Reports retain every frame, run means, `(max - min) / mean * 100` spread and the
worst spread for any corresponding frame/counter. Both must be at most 1%.
Zero-work rows have zero spread only when every observation is zero. Different
frame counts, counter shapes, WASM, bridge or chunk identities fail. Reports
identify the server and client revisions, dirty source diff digests, asset
manifest digest and served artifact/input hashes. Inspect the raw run files
after failures; failed or incomplete captures never earn repeatability credit.

## Fixed workload and boundaries

Browser viewport is 1280 by 720, DPR 1, with the ordinary HUD/canvas layout.
Given map midpoint `(c,c)`, the script holds
eight rendered frames at each of these views:

| Hold | Camera | Zoom |
| --- | --- | ---: |
| 1 | c, c | 1 |
| 2 | c+32, c+16 | 1 |
| 3 | c+32, c+16 | 0.25 |
| 4 | c-32, c-16 | 0.25 |
| 5 | c-32, c-16 | 1 |
| 6 | c, c | 1 |

The fixture disconnects authority and clears gameplay actors, selections and
orders. Map resources and original terrain/native art remain. World dimensions
come from the package, and animation time is fixed at 1000 ms. This is a map
rendering workload, not a saved army, evolving economy or HUD stress test.
Chunks in a bounded halo of the script are fetched in fixed coordinate order
and installed through the existing surface-fixture decoder.

Before each hold, native requests/uploads settle, with five consecutive upload
free callbacks and a successfully rendered scene. The CPU prepared scene is
then discarded, so the first counted frame includes its rebuild and the next
seven exercise reuse. Every counted frame must execute its expected scene
build and submit nonempty draw work. There are exactly six builds and 48 draws.
Native network/decode warmup is outside the metric, like restore/warmup in
[save replay](../scale/save-replay.md). It is not a cold-loading benchmark.

GPU completion is a barrier outside the score: finish, yield through browser
event-loop turns, and poll the real renderer fences until its queue is empty.
WebGL fences do not become observable merely by issuing a synchronous finish.
The barrier prevents queue pressure from skipping frames under different load.
Wait iterations and completion polling are excluded, not converted into work
or latency estimates. GPU execution, fragment cost and driver instructions are
not measured.

After each rendered frame and its completion, a separate upload-pressure probe
writes sixteen identical one-pixel RGBA patches to page 7's final texel using
the native streamer's atlas-batch boundary. It reports checks, calls and bytes
separately from the frame. This controlled probe exercises round one's change
even when all scene textures are resident. It runs only in the private benchmark
artifact and is not a claim about the frequency of uploads in a real match.

| Counter | Meaning |
| --- | --- |
| terrain.demand_tiles | Actual tile-loop visits in native terrain demand, including retries at coarser levels |
| terrain.textured_triangles | Triangles passed through the material-texture assignment loop |
| terrain.cliff_faces | Face inputs examined for cliff demand when native cliff art exists |
| terrain.projected_tiles | Terrain inputs to a new client prepared scene |
| terrain.scene_builds | Client prepared-scene constructions |
| sprites.inputs | Resource and unit inputs to the shared sprite emitter |
| webgl.error_checks | getError calls during the counted callback |
| webgl.texture_uploads / texture_bytes | RGBA8 subimage calls and bytes submitted during that callback |
| webgl.buffer_upload_bytes / buffer_allocated_bytes | Actual instance-buffer uploads and bufferData allocation bytes |
| webgl.draw_calls / instances | Actual instanced draws and submitted instance counts, including extra passes |
| upload_probe.* | Separate controlled burst: checks, patch calls and RGBA bytes |

Rust counters and driver exports are compiled only with `frame-bench`; normal
play has no counter storage, increments or benchmark exports. JavaScript API
wrappers exist only in the browser driver. The measured build pays checked
integer increments and JS wrapper calls, so its latency is not comparable with
an ordinary gameplay build. Counters observe selected work, not all allocations,
all client instructions, browser layout, GPU time or finished RTS performance.

## Historical comparisons

Use the same current server/packages for historical clients: the parent of
round one predates the current map recipe. Build only their instrumented client
artifacts in isolated checkouts, with private Cargo targets:

```sh
git diff dfad610ac7af HEAD --binary -- crates/client crates/rendering > .cache/metric-client-adapter.patch
git worktree add --detach .cache/metric-revisions/parent cca6bbd9da2bafbc467316fe9d386d46f5cbd51b
git worktree add --detach .cache/metric-revisions/round1 3e210b830a572258d660f4fd16152fb0e151881f
git worktree add --detach .cache/metric-revisions/round2 9071ef4b2022358e128c70b4350d224fe5a61f83
```

Apply the adapter to each checkout and stage its client/rendering paths so its
new files appear in the recorded source diff. In the parent adapter only, replace
the probe's unavailable `with_game_atlas_updates` wrapper with the same sixteen
direct `update_game_atlas` calls. Also expose its existing inline fence-drain
loop as a `poll()` method for the unscored completion barrier, leaving the
original render loop unchanged. This is the pre-batching production boundary;
retain the diff outside Git. There are no revision branches in the committed
benchmark. For each checkout (example uses `round1`):

```sh
make client-frame-bench-client-build CLIENT_FRAME_SOURCE="$PWD/.cache/metric-revisions/round1"
make client-frame-bench CLIENT_FRAME_BENCH_ARGS='--package-directory local-assets/map-packages --content-hash 4d9cb16c1c286a5b29ba1d94e70371af966ac7a130a1c3c0f6bcbe95f3b5b683 --asset-pack local-assets/packs/8f7abe8c330d7fd710daa076c8ef07de65f8973feed59b99f4d01f9cbc9c4530 --client-package-directory .cache/metric-revisions/round1/.cache/client-frame-bench/pkg --out .cache/client-frame-bench/round1.json'
```

## Measurements

Measured on dev `dfad610ac7af6e5b0266bfa800cb4d460dffc056` plus the counter
adapter, 2026-11-13. Final raw report: `.cache/client-frame-bench/dev-final.json`.
One final six-trial campaign covers the finished driver and fresh compilation
targets. An earlier bring-up campaign (`dev.json`) also had 0% spread; all its
frame matrices match the final campaign. Failed bring-up/startup attempts are
retained separately and do not count as successful campaigns.

| Condition | Demand visits, runs 1 / 2 / 3 | Textured triangles, runs 1 / 2 / 3 | Probe checks, runs 1 / 2 / 3 | Worst spread |
| --- | ---: | ---: | ---: | ---: |
| Baseline | 224560 / 224560 / 224560 | 142848 / 142848 / 142848 | 48 / 48 / 48 | 0% |
| Make compiling | 224560 / 224560 / 224560 | 142848 / 142848 / 142848 | 48 / 48 / 48 | 0% |

The table uses integer totals over 48 frames. **Every one of the 16 counters in
every corresponding frame was identical across all six trials**, not only the
three selected rows or their totals. Run-mean spread and worst per-frame spread
were both 0%. Compilation output is `dev-final.compile.log` beside the report;
the owned Make compilation remained live before/after all three loaded trials.
Other Make client builds also overlapped the earlier campaign's loaded trials.

| Dev counter | Mean per rendered frame |
| --- | ---: |
| Terrain demand visits | 4678.333333 |
| Textured triangles / cliff-demand face inputs | 2976 / 2976 |
| Projected terrain inputs / scene builds | 584.791667 / 0.125 |
| Sprite inputs | 784.166667 |
| WebGL error checks / texture uploads / texture bytes | 0 / 0 / 0 |
| Buffer upload bytes / allocated bytes | 421456 / 0 |
| Draw calls / submitted instances | 375.166667 / 3763 |
| Separate probe: checks / patches / bytes | 1 / 16 / 64 |

The measured WASM SHA-256 is
`9dcb68f9ed8c27e986843cc53dde1d8dfe849d7f7e1be97db37a67def6891cf5`;
bridge SHA-256 is
`23245cc1ba51b929f4e2942918ea0774b1ae731eb1ae37feaf9c072e53d1e3aa`.
The report retains all input/source digests. The measured adapter is preserved
as `.cache/metric-final-source.patch` and `.cache/metric-final-measured-full.patch`.
Its instrumented WASM is byte-identical before and after final formatting.

### Acceptance comparisons

Each historical revision completed three independent trials with 0% run-mean
and worst per-frame spread across all 16 counters. Reports are `parent.json`,
`round1.json` and `round2.json` beside the dev report.

| Client revision | Probe checks / burst | Demand visits / frame | Textured triangles / frame | Cliff-demand inputs / frame | Draw calls / frame | Buffer upload bytes / frame |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `cca6bbd9d`, round-one parent | 32 | 4678.333333 | 2976 | 2976 | 375.166667 | 421456 |
| `3e210b830`, round one | 1 | 4678.333333 | 2976 | 2976 | 375.166667 | 421456 |
| `9071ef4b2`, round two | 1 | 584.791667 | 372 | 372 | 375.166667 | 421456 |

**Round one is a clear win:** sixteen patches still submit 64 bytes, while
driver error queries fall from 32 to 1 per burst (**96.875% fewer**). Ordinary
scored callbacks report two queries before and zero after; completion-barrier
queries are excluded, so those rows are not total frame-validation counts.

**Round two is a win in the measured terrain work:** demand tile visits,
material assignments and cliff-demand face visits each fall **87.5%**. Every
other counter, including sprite inputs, projected terrain, scene builds, draw
calls, instances and uploaded bytes, stays identical. This confirms retained
view-local work, not an overall FPS, allocation or retained-memory improvement.
The optimization itself remains outside this tooling change.

The original failed round-one startup campaign is retained in
`.cache/metric-round1.log` and `.cache/metric-round1-before-startup-failure.json`.
The corrected driver freezes after native asset initialization instead of
waiting for an unrelated live-game frame. Its first trial's complete counter
matrix is identical to the earlier completed trial. Timeouts were not increased.

## Verification and footage

Dockerized commands used for the final change:

```sh
make hooks-install hooks-check
make hooks-check fmt-check lint structure-check architecture-check docs-check
make fmt test-harness-focused HARNESS_TEST_FILTER=e2e::client_frame
make test-harness-focused HARNESS_TEST_FILTER=gates::registry
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-client AOE_NATIVE_TEST_FILTER='all()'
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-rendering AOE_NATIVE_TEST_FILTER='all()'
make client-frame-bench-format client-frame-bench-browser-test browser-check
make perf-wasm-cap
make test-wasm-focused AOE_WASM_TEST_PACKAGE=aoe-rendering AOE_WASM_TEST_FILTER=game_renderer::world_sprites::village::tests::residency::patch
make test-wasm-focused AOE_WASM_TEST_PACKAGE=aoe-rendering AOE_WASM_TEST_FILTER=native_patch_
```

Results: client native **40/40**, rendering native **11/11** (including counter
reset/addition), benchmark harness **4/4**, registry **35/35**, JavaScript
counter tests **3/3**. The JavaScript cases include actual WebGL2 shader pixels,
texture/buffer uploads and instanced draws, with exact counter and pixel checks.
Hook, formatting, lint, structure, architecture, browser and docs gates pass.
Normal client optimized WASM gzip is **413233 bytes**, +9.9% against the
unchanged 375952-byte baseline and +25% ceiling. Its JS exports contain no
`frame_bench` API. Focused WASM commands returned success but emitted no
individual case summaries; no numeric WASM execution-count claim is made.

Every campaign records its first trial under `.cache/client-frame-bench/video/`;
the actual path is in `runs[0].video`. The final dev recording is VP9, 1280×720,
25 FPS; attach it through `SHIP_VIDEO`, never Git. Initial/midpoint/final script
screenshots show native terrain, original HUD art and the dense forest/river
view. Recording adds load in the first trial; its counters match the five
unrecorded trials exactly. Footage illustrates the workload, not FPS.

Full suites, CI, publication, saved-army/HUD stress, cold loading, allocation/RAM,
GPU timing and dedicated-hardware qualification remain outside this result.
The optional `make describe-check` rejects the local brief's required `medium`
header because this change has a `high` shipping-review floor. The requested
header is preserved; publication would need that conflict resolved. No gate or
review floor was changed.
