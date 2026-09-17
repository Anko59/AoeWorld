# Initial microbenchmark baseline

The values in `micro.json` were captured on 2026-09-17 with the pinned
Rust 1.93.1 analysis image, Gungraun 0.19.4, and Valgrind 3.19.0. ASLR stays
enabled because the local Docker security profile forbids `setarch -R`.
Three repeated local runs produced the same instruction, allocation byte, and
allocation count values for all three kernels. The file binds the smoke
scenario hash to the measurements. This initial baseline is a reviewable
starting point for synthetic kernels, not a game performance qualification.
`wasm.json` records the 81,750-byte gzip size of the Binaryen 108 optimized
WASM bundle, using gzip 1.12 with `-n -9` for repeatable output.

`make perf-baseline-propose` writes fresh measurements to the ignored
`reports/perf/micro-proposal.json`; it does not change this baseline. Review
old and new values, scenario/tool versions, and a written rationale before a
future baseline change. CODEOWNERS protects this directory on a configured
remote. The 5% threshold remains in the tested Rust comparator.
