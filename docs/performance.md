# Performance evidence

`make perf-smoke` runs the 8-client, 8,000-entity scenario through the
authoritative gameplay WebSocket at `/game/ws`, using the production
`GameWorld`. `make perf-ci` adds the 64-client, 128,000-entity
distributed and hotspot scenarios. `make perf-full` adds a comparable
8k/32k/64k/128k population series with 2,000 units per client and fixed seed
and map dimensions. It also adds two sparse-world scenarios: they contain
identical active entities in 1,024² and 16,384² logical maps. The runner
verifies client and population counts, two deltas per
client, hotspot visibility of at least 10,000 sprites, and equal sparse-query
work. Reports are written to ignored `reports/perf/*.json` and Markdown.
`make perf-soak-10` and `make perf-soak-30` repeatedly start the target
server and connect all 64 protocol clients for fixed 10- and 30-minute runs.
They check every cycle's snapshots and deltas, report cumulative encoded
traffic and retained process RSS, and fail if retained RSS exceeds 1 GiB.
Nightly and weekly workflows dispatch the respective durations after the
workflow reaches the default branch.
`make perf-stress` starts the 256,000-entity, 128-client beyond-target
scenario. It requires 127 subscribed clients to receive snapshots and deltas,
and the 20,000-entity hotspot client to receive an explicit protocol-limit
error. This reports bounded overload behavior; it is not a beyond-target
timing or rendering pass. The weekly workflow runs this scenario after it
reaches the default branch.
`make perf-pressure` schedules 10 subscriptions per client at fixed intervals
for a 64-client hotspot scenario. Camera regions and sizes change at every
offer, independently of response completion. The report records every offer,
snapshot latency, missed offer deadline, maximum response backlog, server tick
deadline misses, four reconnects under load, and a paused slow reader. Hosted
timing samples are informational; missing scheduled work or protocol responses
fail the workload.

`make perf-scale` is the headless 100-player, 100,000-unit tick and server
fan-out measurement; its numbers and hot spots are in
[scale measurements](scale/measurements.md).

`make perf-timing` runs pinned Criterion 0.8.2 benchmarks for a synthetic
simulation tick, viewport query, 1,000-entity snapshot encode/decode, and
256-color palette decode. Nightly CI uploads `reports/perf/timing.json` and
`timing.md`. The versioned JSON contains the exact revision, dirty status,
workload identity, toolchain, raw Criterion samples, and estimate intervals.
Its `INCONCLUSIVE` verdict means hosted elapsed time does not establish a
hardware timing pass. Criterion timings never block a merge for regression.

The Playwright hotspot camera test uses a 1920×1080 browser viewport and
collects bounded frame-interval, client decode/update, and CPU render-submit
samples from the Rust/WASM client. It records navigation-relative first-visible
time, WASM memory size, resident and
visible counts, and persistent GPU resources in its JSON attachment. The
synthetic renderer uploads one small mask atlas on initialization and tints it
per player. This test checks that the atlas and other persistent resources are
reused through camera churn and zoom. Browser timings are informational on
shared hosts. The raw samples and browser identity are saved to ignored
`reports/perf/browser.json` and uploaded as CI diagnostics.

The native DAT atlas measurement on 2026-10-20 used trial pack
`0389690a9b3ca887c53aa1afc6ee7f9247f1e63513d07582eaf0d4a56c8fd3a2`.
After applying the client's existing idle/fisherman selections, its 4,859
entries still overflowed the two object pages (7,968,239 object pixels).
Sampling the 60-frame Dock wake at every second frame, with 300ms per retained
frame, preserves its nine-second loop and saves 535,997 pixels. The measured
result is 4,829 entries and 7,432,242 object pixels, packed by
`make building-proof` into the unchanged four 2048-square RGBA8 pages:
67,108,864 bytes (64 MiB). CPU atlas storage and GPU texture payload retain that
same bound; this is not a measurement of driver overhead or dedicated VRAM.
The 5,120-entry limit is unchanged. The strict `make test-assets-pack` full
original-catalog qualification remains unavailable on this trial: its optional
rock 15033 and snow 15026 sheets are absent. Portable complete-catalog tests
and this real available-source measurement are separate evidence.

## Native building atlas

Every building, construction and ambient source packs at full native resolution
(one atlas texel per source pixel); the half-resolution policy is removed. Measured
on 2026-10-21 with `make building-proof` on trial pack
`80d86805bb0ae23d78d26bc64904b068337323d9d18a3d2b1098ecc11dccdc60` merged with its
DE overlay (4,868 selected frames, 1,310 of them terrain). Run it on a pack
directory without symlinked overlay targets (the container does not mount them;
a missing overlay silently drops the `de/` sources and 22 frames).

| Object-domain texels | Frames | Half resolution | Native |
|---|---|---|---|
| Buildings: DAT bodies | 167 | 1,074,659 | 2,556,865 |
| Construction: DAT stages | 15 | 129,827 | 516,263 |
| Construction: fallback foundations | 3 | 32,736 | 130,944 |
| Rubble and Palisade Gate | 19 | 374,896 | 763,216 |
| Feudal supplemental | 1 | 6,888 | 27,552 |
| Dark supplemental and ambient | 77 | 135,687 | 135,687 |
| Fishing | 56 | 79,144 | 79,144 |
| Units | 2,448 | 3,729,060 | 3,729,060 |
| Resources and other objects | 772 | 2,011,223 | 2,011,223 |
| Object total | 3,558 | 7,508,584 | 9,884,418 |

Terrain (its own domain) stays 6,226,430 texels in 1,310 frames. Two object pages
hold 8,388,608 texels, so native resolution overflows them by 1,495,810 texels.
Before the budget moved, the old four pages packed at 82.9, 67.1, 90.7 and 88.3%.

Lossless reclaim was measured first (`make building-proof` prints it): tight
bounding boxes over the colour, player and shadow planes of every selected frame
remove 542,542 texels (17,196 from building bodies, 8,552 from construction
stages), and sharing frames with identical content removes 149,155 more (138,387 in
unit cells, whose team colour the runtime bakes differently per owner). No mirrored
pairs exist. Even with all of it the object domain needs about 9.26M texels, above
the 8.39M of two pages, and a packer that lets objects borrow the terrain pages'
spare rows still overflowed in the browser (the real client selects 9,713,951
object texels). So the budget moved: `PAGE_COUNT` 4 to 5, terrain keeps pages 0-1
and objects take pages 2-4. The packer, the 5,120-entry limit and the 2048 page
side are unchanged. After the change: pages 2-4 pack at 90.6, 92.2 and 52.9%
(9,884,418 of 12,582,912 texels, 2,698,494 free), terrain pages at 82.9 and 67.1%.

Cost of the page: CPU atlas storage and GPU texture payload grow from 67,108,864
to 83,886,080 bytes (+16,777,216 bytes, +25%; a 2048x2048x5 RGBA8 array texture
and one 80 MiB WASM buffer). It is the texel payload only, not a measurement of
driver overhead or dedicated VRAM. The download is unchanged: the source pages
already held full-resolution texels and the old sampling skipped them. Start-up
copies 2.4M more texels (about 9.5 MB); that was not timed separately. The WASM
gzip cap and baseline are unchanged. The two ground sheets this trial lacks (rock
15033, snow 15026, about 950,000 terrain texels) fit the terrain domain's 2,096,642
spare texels by area; no pack with them was available to prove it.

Streaming (2026-10-22): the client loader no longer builds the 80 MiB atlas in
WASM memory. It fills one 16 MiB page buffer at a time and hands each page to
the renderer: WebGPU writes it straight into the texture and reuses the buffer,
so the CPU atlas never exists; WebGL2 and Canvas gather the pages they must keep
(restoration copy, raster source). A source sheet that feeds several pages is
decoded again for each. On the memory spec's webgpu project WASM linear memory
fell from 115.25 to 51.25 MiB and the largest summed Chromium resident size from
about 834 to about 741 MiB (single local runs, noisy by tens of MiB).

Dedicated machines can submit `reports/perf/hardware-environment.json` and
`reports/perf/hardware-samples.json` to `make perf-hardware-check` (or pass
`--environment`, `--samples`, and `--baseline` paths to the Rust CLI). The
environment manifest has `version: 1` and records CPU/GPU models, GPU driver,
OS/kernel, browser name/version, Rust toolchain, a map of full `sha256:`
container digests, memory and CPU limits, CPU governor, GPU power mode,
concurrent job count, and viewport dimensions. The checker requires one
concurrent job and a 1920×1080 viewport. It rejects missing identifiers and
any difference from a reviewed baseline environment.

The versioned sample input records the exact source revision and target-hotspot
workload hash, 128,000 total/resident entities, at least 10,000 visible
sprites, at least 60 seconds of raw tick and frame interval samples, CPU
submission samples, optional GPU timings, and missed tick/dropped frame counts.
It requires at least 20 tick and 60 frame samples per second. A reviewed
`baselines/perf/hardware.json` must contain three distinct sample hashes with
tick and frame p99 values within 5% across runs before a timing result can
pass. The candidate must satisfy 50 ms tick and 16.667 ms frame p99 limits,
zero missed deadlines/dropped frames, and no more than 5% drift from the
baseline median. Missing hardware baseline yields `UNBASELINED`; incompatible
environment or incomplete samples are rejected. The checker retains raw
samples and environment in versioned JSON plus a Markdown summary. No
dedicated hardware or reviewed timing baseline is available yet, so hardware
qualification remains **not established**.

The browser bundle uses the `wasm-release` Cargo profile: size-oriented
optimization, full link-time optimization, and one code generation unit.
`make build-wasm` produces this profile before binding the browser module;
native server and benchmark builds retain their existing profiles. Browser
E2E checks exercise the resulting shipped bundle. The gameplay bundle is
`web/pkg`; the diagnostics app uses `web/diagnostics-pkg`, including its replay
verification kernel. Both are built by `make build-wasm`, and `make perf-wasm-cap`
requires both optimized gzip artifacts to fit the same unchanged baseline cap.
`make perf-wasm-functions` writes per-function Binaryen metrics under `.cache/`
for diagnosing size growth; it does not change the baseline.

The comparison logic has explicit `PASS`, `REGRESSION`, `UNBASELINED`, and
`INCONCLUSIVE` verdicts and a 5% threshold. A missing baseline or sample cannot
pass. The pinned Gungraun/Callgrind and DHAT analysis image measures four
simulation, protocol, and palette-decoding kernels. Binaryen optimizes the WASM bundle before gzip
size comparison. Their reviewed initial baselines live in `baselines/perf/`;
`make perf-baseline-propose` writes proposed replacements to ignored reports
without changing the baselines. The WASM size baseline was re-taken on
2026-10-10 at 228410 B (from 217240 B, set by #6), the size of the tree that
reverts #208 and #221: both existed only to fit a 5% gate that had 37 B left,
one by moving WebGPU out of Rust. It was re-taken on 2026-10-11 at 236275 B,
measured on `dev` ba5417c: the growth is merged product code (world-anchored
terrain texels and their frame directory, #268), and the allowance left could
not hold the next reviewed features. It was re-taken again on 2026-10-12 at
248103 B, measured on `dev` 58dc3ec: ground blending with separate shader
programs (#289, #294), water drawn by depth (#284) and four more ground sheets
(#291) used the whole allowance, leaving `dev` itself at the cap. It was re-taken on
2026-10-14 at 261677 B, measured on `dev` 7a143b5: rivers with banks and fords (#348, #350), blended
shorelines (#349), play feedback (#361) and connected cliff walls (#370) put `dev` itself 1169 B over
the cap, so every pull request failed the comparison. It was re-taken on 2026-10-17 at
274135 B, measured on `dev` 3428ce55: the client batch of HUD (#434), setup-only maps (#451),
visuals (#452) and village controls (#459) left `dev` 625 B under the cap, so the next client
change would fail. It was re-taken on 2026-10-20 at 287702 B, measured on `dev` 33e7ede0
after the Dark Age client batch left `dev` 139 B under the cap (growth measured on each
pull request branch):

| Pull request | Change | Growth |
|---|---|---|
| #516 | Building menu, placement, construction and building panels | about +5 KB |
| #517 | Militia, villager combat and wildlife animations | about +5 KB |
| #519 | Fog of war: explored, fogged and visible tiles | +2.35% |
| #520 | Fishing: fish, the Dock and Fishing Ships | about +7 KB |

On 2026-10-20, the same source tree measured 287584 B with Rust 1.93.1 and
284927 B with Rust 1.99.1 (-0.924%), so the WASM baseline stays at 287702 B;
only its toolchain identity changes. Diagnostics measured 84358 B and 83308 B,
respectively, and retain the shared gameplay cap.

The instruction baselines below were re-taken for the toolchain change, same tree,
using `make perf-instructions` with Gungraun 0.19.4 and Valgrind 3.19.0:

| Kernel | Rust 1.93.1 instructions | Rust 1.99.1 instructions | Baseline action |
|---|---|---|---|
| Palette decode | 238987 | 243938 | Re-taken (+2.07%) |
| Snapshot encode | 207518 | 206498 | Kept at 207527 (within 1%) |
| Simulation advance | 675830 | 712509 | Re-taken (+5.43%) |
| Viewport query | 269812 | 257593 | Re-taken (-4.53%) |

All four allocation byte/count pairs were unchanged: palette 17152/257,
snapshot 16376/11, advance 0/0, and viewport 54024/84. Workload identities,
tool versions, and the 5% regression threshold stay unchanged. These are
deterministic kernel measurements, not gameplay timing qualification.

The next queued client work, DAT building composition (#518, about +3.1 KB) and the Feudal
economy (about +0.77%), fits under the new cap. A baseline moves only in its own reviewed
change, with the reason recorded here. These measured deterministic comparisons pass
locally. Encoded network bytes and tick durations are informational because
they vary with scheduling. The dedicated hardware environment and
qualification are **not established**; 20 Hz simulation and 60 FPS rendering
have not been qualified for real gameplay. Future pathfinding, combat,
economy, and fog of war require representative workloads before acceptance.

Repeated map chunk timings and deterministic ground-variety work bounds are
recorded in [ground variety evidence](geodata/landscape/ground-variety.md).

`make perf-wasm-cap` measures (`make perf-wasm-size`) and then runs
`aoe-harness perf-wasm-size-check`, the same comparison against
`baselines/perf/wasm.json` that `perf-ci` makes, and fails over the cap. It is a
preflight gate for any change that selects the performance suite (a client,
rendering, web or gameplay change, not a docs one), so `make ship` finds a size
overage before CI does (#434 lost a CI cycle to 519 bytes). It measures in its
own gate snapshot, because each gate runs in a separate checkout, and it is
heavy, so its slot and load wait come before its deadline. `make perf-wasm-size`
alone only measures, so `make perf-baseline-propose` still works when the size
grew.

On pull requests whose `target-performance` job ran, CI posts one informational
performance comment and rewrites it in place on later runs (`make perf-comment`).
It shows the optimized WASM size as `baseline …, cap …` (the cap is the size
gate's own comparison threshold applied to `baselines/perf/wasm.json`) and the
pinned kernels' instruction counts as deterministic rows. Tick and
protocol-pressure timings appear only when the PR value falls outside the band
`dev` itself spans across its latest runs whose `target-performance` job passed
and which have a `performance-report` artifact — their `[min, max]` widened by
the factor in `crates/harness/src/perf_comment/noise.json` (25% of the range
over up to 5 runs); with fewer than 3 such runs no timing rows show at all.
The search pages through at most 50 recent `dev` push runs of `ci.yml`, stopping
once enough reports are found. Cancelled or failed runs qualify when their
performance job passed. The CI job durations of the run close the table. The
comment never gates a merge, and it does not measure real frame rate: CI renders
in software and the 1080p check is dedicated-hardware qualification only.

`AOE_MAP_SAMPLES=15 make map-perf` benchmarks a fixed synthetic chunk in release
mode; adding `AOE_MAP_PACKAGE=/path/to/hash.json` selects its prepared center
chunk. Raw samples and provenance go to `reports/perf/map-chunk.json` and `.md`.
Package loading and warmup are excluded; this does not qualify server paging
or dedicated hardware. Without `AOE_MAP_SAMPLES`, the existing five-chunk
fallback sampling remains available.
