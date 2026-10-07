# Correctness breaker

Find inputs or states where the changed code gives a wrong result, panics or
diverges. In this Rust/WASM game look especially at: integer overflow and
lossy casts; off-by-one in grid, tile and chunk math; `unwrap`/`expect` and
indexing reachable at runtime (a panic aborts the WASM instance); float
non-determinism and `HashMap` iteration order in the deterministic simulation;
state-machine transitions; serialization round trips; concurrency and
cancellation in the server and harness; shell and path handling in the harness.
A finding names the triggering input and the line where behaviour goes wrong.
