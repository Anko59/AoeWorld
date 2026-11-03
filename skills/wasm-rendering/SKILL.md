# WASM and rendering

Read [architecture](../../docs/architecture.md), [testing](../../docs/testing.md), and the scoped instructions in `crates/client/AGENTS.md` and `crates/rendering/AGENTS.md`. Check WebGPU capability failures and resource counters. Run focused tests, for example `make test-wasm-focused AOE_WASM_TEST_PACKAGE=aoe-client AOE_WASM_TEST_FILTER=dead_naval_units`, and `make perf-wasm-cap` for client changes. Full e2e and performance benchmarks run in CI.
