# Ground variety generation evidence

The counting contract is
`crates/map/src/terrain/landscape/queries/chunks/tests/variety_cost.rs`.
It counts calls in the actual integer noise and lattice functions, using
thread-local test instrumentation compiled out of production. Each of seventeen
base materials is evaluated over a 32×32 fixture (x −32..−1, y 32..63, seed 17),
for all five height bands and uncleared, grazing and crop land uses.

| Base material | Maximum noise fields/tile | Maximum lattice evaluations/tile | Maximum noise/lattice calls per 1,024 tiles |
| --- | ---: | ---: | ---: |
| Temperate grass, dry grass | 6 | 24 | 6,144 / 24,576 |
| Lush grass, sand, rock | 4 | 16 | 4,096 / 16,384 |
| Forest floor, dirt, mud, snow, ice, shore, water, deep water, Ford, grass/dirt, worn grass, gravel | 0 | 0 | 0 / 0 |

Four wear fields each read four lattice corners. Two lush fields are lazy:
only temperate/dry grass reaching their growth decision evaluates them.
The test requires positive measured work for varied materials, bounds every
tile independently, and requires exactly four lattice calls per noise field.
Shore and impassable tiles skip variety in the composer entirely.

## Repeated production chunk timings

Measured 2026-10-12 with `make map-perf` in release mode, Rust 1.93.1,
on a shared AMD Ryzen 5 4600H host. `AOE_MAP_SAMPLES=15` selects fifteen
samples of sixteen calls to `landscape_chunk_with_cancel`, after sixteen
warmup calls. Returned chunks are consumed and dropped inside the timed batch.
Package parsing/verification, page loading, warmup and report writing are excluded.
`reports/perf/map-chunk.json` and `.md` retain raw nanoseconds and provenance;
comparison copies are in `reports/perf/variety/` (ignored local evidence).

Before generator revision: `900acd826b3fb8d215338cbcd2c43c3701f128fb`.
After generator revision: `2646740f894d5a31934e4800fcaf7be4493b66db`.
Both runs used the identical benchmark-only working-tree overlay; reports
honestly mark them dirty. Subsequent changes add only test counters and evidence,
with no release-generation change. Original variety goldens and low-band digest
`21a6efa9d5326472` remain unchanged.

| Workload | Before median ms/chunk | After median ms/chunk | Change |
| --- | ---: | ---: | ---: |
| Synthetic: key [17;32], seed 1, width 128, chunk (1,1), 634 forest-floor / 390 temperate-grass tiles before | 9.617050 | 9.482932 | −1.39% |
| France: prepared resident pages, chunk (312,312), 1,024 temperate-grass tiles before | 4.037316 | 4.146842 | +2.71% |

France package: `a05dd70e3ca70a04cd37e8dbcdaedcb6d6c6d8d197fac63ab42ceb0e2ed18640`.
Reproduce with `AOE_MAP_SAMPLES=15 make map-perf`, or add
`AOE_MAP_PACKAGE=$PWD/local-assets/map-packages/a05dd70e3ca70a04cd37e8dbcdaedcb6d6c6d8d197fac63ab42ceb0e2ed18640.json`
for France. The increase is within a few percent, so no cache or output-changing
optimization was introduced. Existing unused-material and lazy-lush skips remain.

These medians are informational shared-host observations, not a hardware
regression verdict. Resident prepared pages do not measure the server's page
cache or cold source IO. Neither this fixture nor one real package chunk
qualifies finished RTS performance or dedicated hardware.
