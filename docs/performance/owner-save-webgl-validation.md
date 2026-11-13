# Owner-save WebGL error-query batching

Frame intervals in milliseconds (p50 / p95), including every stall:

| View | Connected reference | After | Reduction p50 / p95 |
|---|---:|---:|---:|
| Base | 150.1 / 250.0 | 100.1 / 175.1 | 33.3% / 30.0% |
| Army | 175.1 / 300.3 | 125.0 / 225.1 | 28.6% / 25.0% |

Overlapping CPU stage spans in milliseconds (p50 / p95); submit is CPU wall time,
not GPU execution time. Stages cannot be added to derive a frame duration.

| View: stage | Connected reference | After |
|---|---:|---:|
| Base: Decode/update | 36.0 / 66.4 | 23.8 / 51.6 |
| Base: UI | 28.9 / 55.1 | 0.3 / 52.9 |
| Base: Sprites | 28.1 / 50.3 | 9.6 / 13.7 |
| Base: Bridge submit | 22.7 / 43.3 | 5.2 / 7.5 |
| Army: Decode/update | 35.6 / 91.2 | 19.2 / 54.5 |
| Army: UI | 34.8 / 63.7 | 0.3 / 53.7 |
| Army: Sprites | 32.6 / 62.4 | 9.3 / 12.9 |
| Army: Bridge submit | 28.6 / 58.1 | 5.8 / 8.4 |

## Method and limits

The original `.cache/fps/before.json` is invalid: both views ended disconnected,
and its army view was empty and unpainted. Its base 150.1/225.2 ms and army
25.0/25.1 ms are retained as evidence, excluded from the win decision.
The connected reference is `.cache/fps/query.json`, measured on base revision
`cca6bbd9da2bafbc467316fe9d386d46f5cbd51b` with the single pre-upload error-query
removal in `.cache/fps/query-runtime.patch` already applied. The comparison
therefore measures the subsequent batching and completed-frame validation work,
not a clean merge-base speedup. Raw after data: `.cache/fps/after.json`.

Both campaigns used headful Chrome 156.0.8078.47, hardware WebGL2, ANGLE AMD Radeon
Graphics (radeonsi renoir ACO), 1920×1080 CSS pixels, DPR 1. Each view followed a
fresh restore of copied `1794535945-mekong-2.save`, matching map, connected native
art, settled chunks and a subsequent painted frame. Base camera: (925,1277),
army: (930,420), both zoom 0.25. Drag selection: (80,100) to (1840,870), pointer
(950,500). Each phase collected all requestAnimationFrame intervals for 120 s;
quantiles use sorted values at Python round((n-1)*p), with no excluded stalls.
The original async restore-wait bug was fixed before the connected reference;
the after helper only adds observation of the starting native frame counter.

Reference/after: base 752/1033 intervals over 120.27/120.45 s; army 647/971 over
120.43/120.27 s. Both valid campaigns remained connected and painted, with no
observed connection changes. Base selection stayed 52; army started at 90 and
ended at 89. After actual presented-frame deltas were 1033 base and 940 army:
the army loop includes 31 intervals without a new native presentation, so these
interval percentiles must not be read as 971 distinct army presentations.

The advancing match and uncontrolled shared-host load limit causal attribution.
Reference host load was higher; decode/update and UI also changed without edits
in those stages. Separate 15 s CDP profiles (outside timing) sampled getError
self time at 3382→591 ms base and 3743→543 ms army. This supports reduced driver
query cost, without attributing every frame improvement to this change.
Neither view reaches 30 FPS; no scale-ladder or dedicated-hardware qualification.
The restore reported 42 lost jobs, no lost entities; uninterrupted-game
equivalence is not established.

Save SHA-256: `1d92110435178df9c870b5c854f7b92983e5f52a8a50faa0ffc18fa236865c0d`.
Reference WASM: `5f337bd45dbd69e296bdf207267ddb56d559c3c3c5eb9deccbc3ceebc6ce5302`.
After WASM: `34fb9ab32d15aa9adde4baa3d6af6ebd7295c5fbdae90f13098ff8dd1807fe4c`.
Measured source hashes: `.cache/fps/after-source.sha256`.

The separate real-time base video is `.cache/showcase/fps-before-after.webm`.
It uses the preserved connected-reference build and measured after build,
with native art and 52 units selected. Capture is outside cadence timing;
CDP clips the bottom of the browser viewport equally in both segments.
No save, asset, raw measurement, image or recording is committed.

## Change and validation

WebGL native upload bursts now query driver errors once on completion; standalone
patches still query after their write. Frame health retires existing fences with
zero timeout, then validates completed submissions, retaining the two-frame queue
bound. Submitted-frame failures stay latched until context restoration/reload.
This changes exceptional error timing, avoiding synchronous queries immediately
after submission. Bounds, resource budgets, geometry and art remain unchanged.
CPU restoration sources still receive uploads during temporary context loss.

Validation uses Dockerized Make and CARGO_PROFILE_DEV_DEBUG=0. Evidence index:
`.cache/fps/stop-recovery.txt`; logs below are under `.cache/fps/`.

- `make fmt-check lint structure-check architecture-check`: pass (`stop-static-final.log`).
- `NEXTEST_TEST_THREADS=2 make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-client AOE_NATIVE_TEST_FILTER='all()'`: 40 pass (`stop-native-client.log`).
- Same focused command with `AOE_NATIVE_TEST_PACKAGE=aoe-rendering`: 10 pass (`stop-native-rendering.log`).
- With `AOE_E2E_PORT_RANGE=8196-8197`, `make test-wasm-focused AOE_WASM_TEST_PACKAGE=aoe-rendering AOE_WASM_TEST_FILTER=residency::patch`: three new browser regressions pass (`stop-wasm-rendering-final.log`). Tests pin standalone/batched query counts, real injected upload errors, failed-batch release, deferred validation and latched errors.
- Same WASM target: client `verified_native_chunks_upload`, renderer `webgl_contours_return_after_context_loss` and `webgl_landscape_parity_fixture`: pass (`stop-wasm-client.log`, `stop-wasm-context.log`, `stop-wasm-parity.log`).
- `make perf-wasm-cap`: pass, gzip 413110 bytes versus unchanged 375952 (+9.9%, ceiling +25%; `stop-wasm-cap.log`).
- `AOE_E2E_WORKERS=1 AOE_E2E_PORT_RANGE=8196-8197 make test-e2e AOE_E2E_SPECS='tests/village/input/performance.play.spec.ts tests/rendering/recovery/backpressure.play.spec.ts tests/rendering/recovery/device.play.spec.ts'`: 24/25 in isolated fixtures; one startup timeout with 13 ERR_NETWORK_CHANGED module requests (`stop-e2e-first.json`, `stop-e2e-first-traces/`, `stop-e2e-isolated.log`). Initial staging also rejected the copied owner manifest at its existing size bound; original data was preserved, then restored after fixture isolation.
- One bounded unchanged-source confirmation with only `tests/village/input/performance.play.spec.ts`: 16/16 pass, no skips/flaky cases (`stop-e2e-performance-confirm.json`, `stop-e2e-performance-confirm.log`). This does not establish that host network instability is fixed.

Full suites and CI have not run. No gate, assertion or baseline was weakened.
No publication was requested; local commit only. The single after campaign met
the ≥20% threshold in both views. No second campaign or new idea was started.

Final pre-commit confirmation on the same measured production source:
`make hooks-install hooks-check fmt-check lint structure-check docs-check` passed
(`commit-static.log`); focused native client/rendering reruns passed 40/40 and
10/10 (`commit-native-client.log`, `commit-native-rendering.log`);
`make perf-wasm-cap` passed with the same sizes (`commit-wasm-cap.log`).
`make video-probe SHIP_VIDEO=/home/anko/Work/projects/AoeWorld-cx-fps/.cache/showcase/fps-before-after.webm`
passed: VP9, 1280×720, 23.90 s (`video-probe.log`). Extracted frames at 2, 14 and
23 s were visually inspected: matching base, native art, selection and labels.
The private development server was stopped after recording.
