# Performance changes

Read [performance](../../docs/performance.md) and [benchmark ADR](../../docs/adr/0005-benchmarks.md). Keep workload identities and counts explicit. Use focused tests for touched crates before committing; CI runs `make perf-smoke` and `make perf-ci` for performance changes. Use `make perf-baseline-propose` for reviewable baseline changes; never weaken a threshold to obtain a pass.
