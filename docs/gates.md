# Implemented gate registry

Generated from `gates/registry.json`. `make docs-check` detects drift.

Registry v2 drives selection and dependency plans. Cadences and budgets are planned runner metadata until shared execution is wired; existing Make/CI dispatch and minimum preflight remain mandatory. No automatic agent interception is implied.

| Gate | Command | Depends on | Suites | Cadences | Budget (s) | Evidence |
|---|---|---|---|---|---|---|
| fmt-check | `make fmt-check` | — | static | [Edit, Stop, Commit, Pr, Ci, Nightly, Preflight] | 120 | rustfmt exit status |
| structure-check | `make structure-check` | — | static | [Edit, Stop, Commit, Pr, Ci, Nightly, Preflight] | 120 | tracked file and directory audit |
| architecture-check | `make architecture-check` | — | native | [Stop, Commit, Pr, Ci, Nightly, Preflight] | 180 | Cargo metadata and source boundary audit |
| docs-check | `make docs-check` | — | static | [Edit, Stop, Commit, Pr, Ci, Nightly, Preflight] | 120 | registry table and local link validation |
| lint | `make lint` | fmt-check | native | [Stop, Commit, Pr, Ci, Nightly, Preflight] | 600 | Clippy exit status |
| deny | `make deny` | — | native | [Stop, Commit, Pr, Ci, Nightly, Preflight] | 600 | Cargo advisory, source, license, and bans audit |
| test-unit | `make test-unit` | lint | native | [Pr, Ci, Nightly, Preflight] | 900 | native unit, integration, and doctest results |
| coverage | `make coverage` | test-unit, build-wasm | native | [Pr, Ci, Nightly] | 3600 | LCOV production-line thresholds and JSON report |
| fuzz-smoke | `make fuzz-smoke` | test-unit | native, assets, release | [Pr, Ci, Nightly] | 1800 | 512 bounded libFuzzer runs per parser and JSON report |
| fuzz-nightly | `make fuzz-nightly` | fuzz-smoke | native, assets, release | [Nightly] | 5400 | 300-second libFuzzer campaign per parser and JSON report |
| mutation-nightly | `make mutation-nightly` | test-unit | everything | [Nightly] | 5400 | pinned comparator and CI-selection mutation outcomes |
| browser-check | `make browser-check` | — | browser | [Stop, Commit, Pr, Ci, Nightly] | 600 | TypeScript, ESLint, and Prettier results |
| test-wasm | `make test-wasm` | test-unit, browser-check, build-wasm | browser | [Pr, Ci, Nightly] | 1800 | wasm-bindgen-test in pinned Chromium and JSON report |
| test-e2e | `make test-e2e` | test-unit, browser-check, build-wasm | browser | [Pr, Ci, Nightly] | 2400 | Playwright result and screenshots |
| perf-smoke | `make perf-smoke` | test-unit | native | [Pr, Ci, Nightly, Preflight] | 600 | versioned synthetic JSON report |
| perf-ci | `make perf-ci` | perf-smoke, perf-instructions, perf-wasm-size | performance | [Pr, Ci, Nightly] | 1800 | target counts and baseline comparisons |
| perf-timing | `make perf-timing` | perf-ci | performance | [Nightly] | 900 | informational Criterion raw samples and versioned report |
| perf-pressure | `make perf-pressure` | perf-ci | performance | [Pr, Ci, Nightly] | 1800 | clock-scheduled subscriptions, backlog, slow reader, and reconnect report |
| perf-stress | `make perf-stress` | perf-ci | performance | [Weekly] | 2700 | 256k-entity protocol load and explicit overload report |
| perf-soak-10 | `make perf-soak-10` | perf-ci | performance | [Nightly] | 2700 | 10-minute target network lifecycle and retained-memory report |
| perf-soak-30 | `make perf-soak-30` | perf-ci | performance | [Weekly] | 4200 | 30-minute target network lifecycle and retained-memory report |
| perf-hardware-check | `make perf-hardware-check` | perf-ci | performance | [Qualification] | 7200 | environment-bound 1080p timing samples and three-run baseline |
| release-build | `make release-build` | test-e2e, perf-ci | release | [] | 7200 | verified local image IDs, bundle hash, and source evidence |
| release-verify | `make release-verify` | release-build | release | [] | 1800 | exact local images, bundle, and report hashes |
| release-publish | `make release-publish` | release-verify | release | [] | 3600 | registry digests, SBOM hashes, bundle export, and GitHub outputs |
| release-rehearse | `make release-rehearse` | release-verify | release | [] | 5400 | local exact-image promotion and rollback health checks |
| release-source-check | `make release-source-check` | ci-check | release | [] | 600 | release branch SHA, dev ancestry, and exact source-tree match |
| release-main-source-check | `make release-main-source-check` | ci-check | release | [] | 600 | unique dev commit whose tree equals the promoted main tree |
| release-verify-published | `make release-verify-published` | release-source-check | release | [] | 1800 | published manifest, evidence, SBOM, bundle, and registry digest verification |
| release-smoke-published | `make release-smoke-published` | release-verify-published | release | [] | 3600 | initial published candidate health; rollback unavailable |
| release-rehearse-published | `make release-rehearse-published` | release-verify-published | release | [] | 5400 | previous, candidate, and rollback stacks use exact published digests |
| ci-select | `make ci-select` | — | everything | [] | 120 | revision-bound job selection manifest |
| ci-check | `make ci-check` | ci-select | everything | [] | 120 | selected job results and manifest recomputation |
| build-wasm | `make build-wasm` | — | browser | [Stop, Pr, Ci, Nightly, Preflight] | 900 | wasm-release client and wasm-bindgen web package |
| perf-instructions | `make perf-instructions` | — | performance | [Pr, Ci, Nightly] | 1800 | pinned Gungraun/Callgrind kernel NDJSON |
| perf-wasm-size | `make perf-wasm-size` | build-wasm | performance | [Pr, Ci, Nightly] | 300 | optimized and deterministic gzip WASM artifacts |
| perf-full | `make perf-full` | perf-instructions, perf-wasm-size, perf-smoke | performance | [Nightly] | 3600 | population and sparse-world scaling counts and existing comparisons |
| test-memory-source | `make test-memory-source` | build-wasm, browser-check | browser, assets | [Qualification] | 3600 | activated real-pack source map shared-surface browser route |
| test-creator-source | `make test-creator-source` | build-wasm, browser-check, test-unit | browser | [Qualification] | 5400 | overview and detailed source maps across restart |
| test-geographic-matrix | `make test-geographic-matrix` | test-unit | browser | [Qualification] | 5400 | prepared and verified fixed overview source regions |
| test-geographic-visuals | `make test-geographic-visuals` | build-wasm, browser-check, test-unit | browser | [Qualification] | 5400 | water and Alpine source packages on WebGPU and Canvas |
