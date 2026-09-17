# Initial microbenchmark baseline

The values in `micro.json` were captured on 2026-09-17 with the pinned
Rust 1.93.1 analysis image, Gungraun 0.19.4, and Valgrind 3.19.0. ASLR stays
enabled because the local Docker security profile forbids `setarch -R`.
Three repeated local runs produced the same instruction, allocation byte, and
allocation count values for all three kernels. The file binds the smoke
scenario hash to the measurements. This initial baseline is a reviewable
starting point for synthetic kernels, not a game performance qualification.
The fourth case, a bounded 256-color JASC palette decode, was added on the
same pinned image. Three separate runs each measured 238,987 instructions,
17,152 allocated bytes, and 257 allocations. The proposal changed only the
case set: the original three values, tool identities, and workload hash stayed
identical. This extends the required comparison to asset decoding.
`wasm.json` records the 81,750-byte gzip size of the Binaryen 108 optimized
WASM bundle, using gzip 1.12 with `-n -9` for repeatable output.

`make perf-baseline-propose` writes fresh measurements to the ignored
`reports/perf/micro-proposal.json`; it does not change this baseline. Review
old and new values, scenario/tool versions, and a written rationale before a
future baseline change. CODEOWNERS protects this directory on a configured
remote. The 5% threshold remains in the tested Rust comparator.
