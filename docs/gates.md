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
| fuzz-smoke | `make fuzz-smoke` | test-unit | code | 512 bounded libFuzzer runs per parser and JSON report |
| fuzz-nightly | `make fuzz-nightly` | fuzz-smoke | nightly | 300-second libFuzzer campaign per parser and JSON report |
| mutation-nightly | `make mutation-nightly` | test-unit | nightly | pinned comparator and CI-selection mutation outcomes |
| browser-check | `make browser-check` | — | browser | TypeScript, ESLint, and Prettier results |
| test-wasm | `make test-wasm` | test-unit, browser-check | browser | wasm-bindgen-test in pinned Chromium and JSON report |
| test-e2e | `make test-e2e` | test-unit, browser-check | browser | Playwright result and screenshots |
| perf-smoke | `make perf-smoke` | test-unit | code | versioned synthetic JSON report |
| perf-ci | `make perf-ci` | perf-smoke | performance | target counts and baseline comparisons |
| perf-pressure | `make perf-pressure` | perf-ci | performance | clock-scheduled subscriptions, backlog, slow reader, and reconnect report |
| perf-stress | `make perf-stress` | perf-ci | weekly | 256k-entity protocol load and explicit overload report |
| perf-soak-10 | `make perf-soak-10` | perf-ci | nightly | 10-minute target network lifecycle and retained-memory report |
| perf-soak-30 | `make perf-soak-30` | perf-ci | weekly | 30-minute target network lifecycle and retained-memory report |
| perf-hardware-check | `make perf-hardware-check` | perf-ci | dedicated hardware | environment-bound 1080p timing samples and three-run baseline |
| ci-select | `make ci-select` | — | CI | revision-bound job selection manifest |
| ci-check | `make ci-check` | ci-select | CI | selected job results and manifest recomputation |
