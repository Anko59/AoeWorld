# Performance changes

Read [performance](../../docs/performance.md) and [benchmark ADR](../../docs/adr/0005-benchmarks.md). Keep workload identities and counts explicit. Run `make perf-smoke` for focused work and `make perf-ci` before accepting changed hot paths. Use `make perf-baseline-propose` for reviewable baseline changes; never weaken a threshold to obtain a pass.
