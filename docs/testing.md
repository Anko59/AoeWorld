# Testing

Use `make fmt-check`, `make structure-check`, `make lint`, and `make test-unit`
for focused checks. `make pre-commit` runs static gates; `make preflight` adds
native tests through pinned cargo-nextest, followed by separate doctests and a
WASM build. `make test-wasm` runs Rust `wasm-bindgen-test` cases in pinned
headless Chromium through a disposable ChromeDriver container. It writes
`reports/wasm/browser.json`; a successful WASM compilation alone does not pass
this gate. `make test-e2e` runs both an explicit WebGPU project under Xvfb with Vulkan
SwiftShader and a game-only project with default browser launch settings.
Game checks cover rendered pixels, movement, resize, and compatibility startup
after a missing WebGPU API, null WebGPU context, or unavailable adapter.
The WebGPU project also checks actual canvas background and four sprite colors in a full-page
screenshot, along with reconnect and independent subscriptions.
`make coverage` records LCOV and JSON, excludes inline test modules from the
production-line denominator, and rejects less than 85% overall or 90% in
protocol, asset parsers, and policy/report logic. A missing group fails.
The coverage run includes both instrumented Playwright E2E and the browser
WASM runner. Browser, WASM, asset, performance, QA, and artifact gates need
separate evidence.
`make fuzz-smoke` runs 512 libFuzzer cases against each DRS, SLP, palette, and
pack-manifest parser. It uses a separately pinned nightly toolchain and keeps
new corpus inputs and crash artifacts outside Git. `make fuzz-nightly` gives
each target a 300-second campaign. Both write versioned reports under
`reports/fuzz/`; a discovered crash fails the gate.
`make mutation-nightly` runs pinned cargo-mutants against the impact
classifier, CI selection check, and 5% performance comparator. It requires a
completed, nonempty campaign with no missed or timed-out mutations and writes
`reports/mutation/nightly.json`. Mutations that cannot compile are reported
as unviable, separately from caught mutations.
Pull-request CI runs `make ci-select` against the base commit and records the
selected jobs in `reports/gates/selection.json`. Unknown paths and missing
base information select every job. The aggregate `required` check recomputes
the selection at the checked-out revision and requires success for each selected
job; only unselected jobs may be skipped. `make ci-check` validates that
contract locally when supplied the manifest and job results.
Do not claim a complete gate until they exist and pass at the exact revision.
