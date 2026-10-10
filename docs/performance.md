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
E2E checks exercise the resulting shipped bundle.

The comparison logic has explicit `PASS`, `REGRESSION`, `UNBASELINED`, and
`INCONCLUSIVE` verdicts and a 5% threshold. A missing baseline or sample cannot
pass. The pinned Gungraun/Callgrind and DHAT analysis image measures four
simulation, protocol, and palette-decoding kernels. Binaryen optimizes the WASM bundle before gzip
size comparison. Their reviewed initial baselines live in `baselines/perf/`;
`make perf-baseline-propose` writes proposed replacements to ignored reports
without changing the baselines. These measured deterministic comparisons pass
locally. Encoded network bytes and tick durations are informational because
they vary with scheduling. The dedicated hardware environment and
qualification are **not established**; 20 Hz simulation and 60 FPS rendering
have not been qualified for real gameplay. Future pathfinding, combat,
economy, and fog of war require representative workloads before acceptance.
