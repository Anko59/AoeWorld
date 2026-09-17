# Implemented gate registry

Generated from `gates/registry.json`. `make docs-check` detects drift.

| Gate | Command | Depends on | Selection | Evidence |
|---|---|---|---|---|
| fmt-check | `make fmt-check` | — | all | rustfmt exit status |
| structure-check | `make structure-check` | — | all | tracked file and directory audit |
| architecture-check | `make architecture-check` | — | code | Cargo metadata and source boundary audit |
| docs-check | `make docs-check` | — | all | registry table and local link validation |
| lint | `make lint` | fmt-check | code | Clippy exit status |
| deny | `make deny` | — | code | Cargo advisory, source, license, and bans audit |
| test-unit | `make test-unit` | lint | code | native unit, integration, and doctest results |
| coverage | `make coverage` | test-unit | code | LCOV production-line thresholds and JSON report |
| browser-check | `make browser-check` | — | browser | TypeScript, ESLint, and Prettier results |
| test-wasm | `make test-wasm` | test-unit, browser-check | browser | wasm-bindgen-test in pinned Chromium and JSON report |
| test-e2e | `make test-e2e` | test-unit, browser-check | browser | Playwright result and screenshots |
| perf-smoke | `make perf-smoke` | test-unit | code | versioned synthetic JSON report |
| perf-ci | `make perf-ci` | perf-smoke | performance | target counts and baseline comparisons |
