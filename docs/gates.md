# Implemented gate registry

Generated from `gates/registry.json`. `make docs-check` detects drift.

Registry v2 drives selection and dependency plans. The opt-in gate-run CLI executes cadence or complete CI-job plans with explicit budgets and local evidence. Existing Make/CI dispatch and minimum preflight remain mandatory until protected judging replaces bootstrap execution. No automatic agent interception is implied.

**Resources and long jobs.** There is no ticket queue or CPU quota. Every build container takes one of a few build slots; long jobs also hold one of a small fixed number of long-job slots.
Every build/test container uses the shared `gates/container.args` policy: 4 GiB memory including swap, relative CPU shares, and four Cargo build jobs. The harness's own Docker commands use the same caps.
Before starting a container, the Rust launcher refuses immediately if host MemAvailable is below 3 GiB; the cap bounds one container, while admission checks the current host headroom. Concurrent admissions are observations, not memory reservations.
A kernel OOM kill is reported as exceeding the container memory cap. Final cgroup memory.peak readings accompany Make container output.
Only gates marked `long` (benchmarks, full E2E, coverage, and aggregates that run them) take one of N nonblocking kernel-lock slots, the files `<git common dir>/aoe-ship/long-0.lock` … `long-{N-1}.lock`; N is 3 unless `AOE_LONG_SLOTS` (1..=8) says otherwise. Focused `test-e2e` with nonempty `AOE_E2E_SPECS` is short; WASM size analysis is short too.
A long job takes the first free slot; only when every slot is held does it refuse with exit 75 and `busy: slot 0: <holder command, branch, worktree, started>; slot 1: …, retry later`, listing every holder; it never waits. Make itself reports recipe failures as exit 2. Short jobs never take this lock.
Every `docker run` that Make starts through the launcher holds one build slot, the files `<git common dir>/aoe-ship/cpu-0.lock` … `cpu-{N-1}.lock`; N is 4 unless `AOE_CPU_SLOTS` (1..=16) says otherwise, about 16 compilers on the 12-core host. One worktree may hold 2 build slots, and one coordinator with all its agents 3 (`AOE_COORDINATOR`, set by agent launchers). A build container waits up to 90 s for a slot, then refuses with exit 75 naming every holder; it never queues longer. A container inside a long job, or inside an admitted container (`AOE_CPU_TOKEN`), uses its parent's slot. Each admission, refusal and finish appends one JSON line to `<git common dir>/aoe-ship/ledger.jsonl`.
Exec children inherit the lock descriptor, so killing the parent does not free it while those children run. Nested targets carry `AOE_LONG_JOB`, `AOE_LONG_DIR` and `AOE_LONG_SLOT` (the held slot), accepting ownership only while that slot's kernel lock and proof are live. Snapshots use the source repository's lock directory.
The host job wrapper takes the lock before running long-target prerequisites. The independent std-only resource launcher bootstraps in a small 256 MiB container; workspace compilation is admitted and capped, without a machine-wide bootstrap lock.
CI uses the same caps and immediate admission; each job has its own runner. `make load-status` shows MemAvailable, every long-job and build slot with its holder, and the latest ledger lines. No wait-time environment variables remain.

| Gate | Command | Depends on | Suites | Cadences | Budget (s) | Long job | Evidence |
|---|---|---|---|---|---|---|---|
| check-fixtures-report | `make check-fixtures-report` | — | map, gameplay | [Preflight] | 120 | — | documented local packages and saves load; a failure is reported as a warning, missing inputs are skipped |
| fmt-check | `make fmt-check` | — | static | [Edit, Stop, Commit, Pr, Ci, Nightly, Preflight] | 120 | — | rustfmt exit status |
| structure-check | `make structure-check` | — | static | [Edit, Stop, Commit, Pr, Ci, Nightly, Preflight] | 120 | — | tracked file and directory audit |
| architecture-check | `make architecture-check` | — | native | [Stop, Commit, Pr, Ci, Nightly, Preflight] | 180 | — | Cargo metadata and source boundary audit |
| docs-check | `make docs-check` | — | static | [Edit, Stop, Commit, Pr, Ci, Nightly, Preflight] | 120 | — | registry table and local link validation |
| lint | `make lint` | fmt-check | native | [Stop, Commit, Pr, Ci, Nightly, Preflight] | 600 | — | Clippy exit status |
| deny | `make deny` | — | native | [Stop, Commit, Pr, Ci, Nightly, Preflight] | 600 | — | Cargo advisory, source, license, and bans audit |
| test-unit | `make test-unit` | lint | native | [Pr, Ci, Nightly] | 900 | — | native unit, integration, and doctest results |
| coverage | `make coverage` | test-unit, build-wasm | native | [Ci, Nightly] | 2700 | yes | LCOV production-line counts and JSON report |
| fuzz-smoke | `make fuzz-smoke` | test-unit | native, assets, release | [Pr, Ci, Nightly] | 1800 | — | 512 bounded libFuzzer runs per parser and JSON report |
| fuzz-nightly | `make fuzz-nightly` | fuzz-smoke | native, assets, release | [Nightly] | 5400 | — | 300-second libFuzzer campaign per parser and JSON report |
| browser-check | `make browser-check` | — | browser | [Stop, Commit, Pr, Ci, Nightly] | 600 | — | TypeScript, ESLint, and Prettier results |
| test-wasm | `make test-wasm` | test-unit, browser-check, build-wasm | browser | [Pr, Ci, Nightly] | 1800 | — | wasm-bindgen-test in pinned Chromium and JSON report |
| test-e2e | `make test-e2e` | test-unit, browser-check, build-wasm | browser | [Pr, Ci, Nightly] | 2400 | yes | Playwright result and screenshots |
| perf-smoke | `make perf-smoke` | test-unit | native | [Pr, Ci, Nightly] | 600 | yes | versioned synthetic JSON report |
| perf-ci | `make perf-ci` | perf-smoke, perf-instructions, perf-wasm-size | performance | [Pr, Ci, Nightly] | 1800 | yes | target counts and baseline comparisons |
| perf-scale-smoke | `make perf-scale-smoke` | — | performance | [Nightly] | 900 | yes | 4-player, 2,000-unit, 50-tick run of the scale benchmark writing valid JSON |
| quality-metrics | `make quality-metrics` | — | performance | [] | 3600 | yes | seeded integer gameplay quality reports; exact current-run repeatability under idle and compiling load |
| client-frame-bench | `make client-frame-bench` | — | performance | [] | 3600 | yes | fixed camera script integer work counters; three idle and optional three compiling runs |
| perf-scale | `make perf-scale` | — | performance | [] | 3600 | yes | per-phase tick times, memory and server fan-out for 100 players and 100k units, as JSON |
| perf-groups | `make perf-groups` | — | performance | [] | 1800 | yes | movement phase times and movement span split for about 11,000 movers in 200 armies, as JSON |
| perf-pressure | `make perf-pressure` | perf-ci | performance | [Pr, Ci, Nightly] | 1800 | yes | clock-scheduled subscriptions, backlog, slow reader, and reconnect report |
| perf-stress | `make perf-stress` | perf-ci | performance | [Weekly] | 2700 | yes | 256k-entity protocol load and explicit overload report |
| perf-soak-10 | `make perf-soak-10` | perf-ci | performance | [Nightly] | 2700 | yes | 10-minute target network lifecycle and retained-memory report |
| perf-soak-30 | `make perf-soak-30` | perf-ci | performance | [Weekly] | 4200 | yes | 30-minute target network lifecycle and retained-memory report |
| perf-hardware-check | `make perf-hardware-check` | perf-ci | performance | [Qualification] | 7200 | — | environment-bound 1080p timing samples and three-run baseline |
| release-build | `make release-build` | test-e2e, perf-ci | release | [] | 7200 | yes | verified local image IDs, bundle hash, and source evidence |
| release-verify | `make release-verify` | release-build | release | [] | 1800 | — | exact local images, bundle, and report hashes |
| release-publish | `make release-publish` | release-verify | release | [] | 3600 | — | registry digests, SBOM hashes, bundle export, and GitHub outputs |
| release-rehearse | `make release-rehearse` | release-verify | release | [] | 5400 | — | local exact-image promotion and rollback health checks |
| release-source-check | `make release-source-check` | ci-check | release | [] | 600 | — | release branch SHA, dev ancestry, and exact source-tree match |
| release-main-source-check | `make release-main-source-check` | ci-check | release | [] | 600 | — | unique dev commit whose tree equals the promoted main tree |
| release-verify-published | `make release-verify-published` | release-source-check | release | [] | 1800 | — | published manifest, evidence, SBOM, bundle, and registry digest verification |
| release-smoke-published | `make release-smoke-published` | release-verify-published | release | [] | 3600 | — | initial published candidate health; rollback unavailable |
| release-rehearse-published | `make release-rehearse-published` | release-verify-published | release | [] | 5400 | — | previous, candidate, and rollback stacks use exact published digests |
| ci-select | `make ci-select` | — | everything | [] | 120 | — | revision-bound job selection manifest |
| ci-check | `make ci-check` | ci-select | everything | [] | 120 | — | selected job results and manifest recomputation |
| build-wasm | `make build-wasm` | — | browser | [Stop, Pr, Ci, Nightly, Preflight] | 900 | — | wasm-release client and wasm-bindgen web package |
| perf-instructions | `make perf-instructions` | — | performance | [Pr, Ci, Nightly] | 1800 | yes | pinned Gungraun/Callgrind kernel NDJSON |
| perf-wasm-size | `make perf-wasm-size` | build-wasm | performance | [Pr, Ci, Nightly] | 300 | — | optimized and deterministic gzip WASM artifacts |
| perf-wasm-cap | `make perf-wasm-cap` | perf-wasm-size | performance | [Preflight] | 300 | — | optimized gzip WASM, measured in this gate's snapshot, reported against baselines/perf/wasm.json; blocks only a jump over +25% |
| perf-full | `make perf-full` | perf-instructions, perf-wasm-size, perf-smoke | performance | [Nightly] | 3600 | yes | population and sparse-world scaling counts and existing comparisons |
| test-memory-source | `make test-memory-source` | build-wasm, browser-check | browser, assets | [Qualification] | 3600 | — | activated real-pack source map shared-surface browser route |
| test-creator-source | `make test-creator-source` | build-wasm, browser-check, test-unit | browser | [Qualification] | 5400 | — | overview and detailed source maps across restart |
| test-source-qualification | `make test-source-qualification` | test-unit | native | [Ci, Nightly, Qualification] | 1800 | — | slow offline source-qualification movement, replay and package tests that test-unit excludes |
| test-exhaustive | `make test-exhaustive` | test-unit | native | [Ci, Nightly] | 1800 | — | 16-seed landscape sweeps and full-axis geodata round-trips that test-unit runs on two seeds or reduced axes |
| sdk-python-test | `make sdk-python-test` | test-unit | native, ai | [Ci, Nightly] | 3600 | — | pytest results of the Python bindings, the example AI decision logic, the standard start and a seeded Dark-to-Feudal game between bots on the AI arena (five seeds) |
| test-geographic-matrix | `make test-geographic-matrix` | test-unit | browser | [Qualification] | 5400 | — | prepared and verified fixed overview source regions |
| test-geographic-visuals | `make test-geographic-visuals` | build-wasm, browser-check, test-unit | browser | [Qualification] | 5400 | — | water and Alpine source packages on WebGPU and Canvas |
| preflight | `make preflight` | — | performance | [] | 3600 | yes | local checks, unit tests and smoke benchmark |
| preflight-unit | `make preflight-unit` | — | performance | [] | 1800 | yes | native unit tests and smoke benchmark |
| perf-late-game | `make perf-late-game` | — | performance | [] | 1800 | yes | late-game profile measurements |
| perf-late-game-instructions | `make perf-late-game-instructions` | — | performance | [] | 1800 | yes | late-game Callgrind instruction report |
| perf-groups-instructions | `make perf-groups-instructions` | — | performance | [] | 1800 | yes | group-movement Callgrind instruction report and per-stage instruction and LL-miss counters |
| perf-groups-profile | `make perf-groups-profile` | — | performance | [] | 3600 | yes | qualified movement timing and instruction reports |
| scale-match | `make scale-match` | — | performance | [] | 3600 | yes | real server, AI hosts and browser scale measurements |
| perf-baseline-propose | `make perf-baseline-propose` | — | performance | [] | 3600 | yes | benchmark observations and baseline proposal |
| scale-match-smoke | `make scale-match-smoke` | — | performance | [] | 1800 | yes | bounded real server, AI and browser scale smoke measurement |
