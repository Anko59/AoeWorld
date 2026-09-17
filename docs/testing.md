# Testing

Use `make fmt-check`, `make structure-check`, `make lint`, and `make test-unit`
for focused checks. `make pre-commit` runs static gates; `make preflight` adds
native tests through pinned cargo-nextest, followed by separate doctests and a
WASM build. `make test-wasm` runs Rust `wasm-bindgen-test` cases in pinned
headless Chromium through a disposable ChromeDriver container. It writes
`reports/wasm/browser.json`; a successful WASM compilation alone does not pass
this gate. `make test-e2e` separately exercises rendering and reconnect with
Playwright.
`make coverage` records LCOV and JSON, excludes inline test modules from the
production-line denominator, and rejects less than 85% overall or 90% in
protocol, asset parsers, and policy/report logic. A missing group fails.
The coverage run includes both instrumented Playwright E2E and the browser
WASM runner. Browser, WASM, asset, performance, QA, and artifact gates need
separate evidence.
Do not claim a complete gate until they exist and pass at the exact revision.
