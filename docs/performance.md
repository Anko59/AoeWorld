# Performance evidence

`make perf-smoke` runs the 8-client, 8,000-entity scenario through actual local
WebSocket connections. `make perf-ci` adds the 64-client, 128,000-entity
distributed and hotspot scenarios. `make perf-full` adds a comparable
8k/32k/64k/128k population series with 2,000 units per client and fixed seed
and map dimensions. It also adds two sparse-world scenarios: they contain
identical active entities in 1,024² and 16,384² logical maps. The runner
verifies client and population counts, two deltas per
client, hotspot visibility of at least 10,000 sprites, and equal sparse-query
work. Reports are written to ignored `reports/perf/*.json` and Markdown.

The comparison logic has explicit `PASS`, `REGRESSION`, `UNBASELINED`, and
`INCONCLUSIVE` verdicts and a 5% threshold. A missing baseline or sample cannot
pass. The pinned Gungraun/Callgrind and DHAT analysis image measures three
simulation/protocol kernels. Binaryen optimizes the WASM bundle before gzip
size comparison. Their reviewed initial baselines live in `baselines/perf/`;
`make perf-baseline-propose` writes proposed replacements to ignored reports
without changing the baselines. These measured deterministic comparisons pass
locally. Encoded network bytes and tick durations are informational because
they vary with scheduling. The dedicated hardware environment and
qualification are **not established**; 20 Hz simulation and 60 FPS rendering
have not been qualified for real gameplay. Future pathfinding, combat,
economy, and fog of war require representative workloads before acceptance.
