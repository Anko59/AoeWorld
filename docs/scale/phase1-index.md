# Phase-1 index port: paired measurement

The four storage ports preserve the current world hashes: mover IDs use a bitset,
changed IDs use dirty bits, presence buckets use a flat array, and collision
cells use sparse flat grids. No hash pin, gate or baseline was changed.

## Revisions and observation

The comparison is frozen on dev `c5f3f0afa34d1a1e0ef8d177c7e48a1dbecbb077`.
The baseline is `ccad7ae997598d99df343afa066f7ab99ce69dfd`: that dev revision
plus the timing observer, with the original indexes. The indexed revision is
`4a27e6520ec540d525bbb0faa57f33d91252c665`. Both measured trees were clean.
Dev subsequently advanced to `8ca8d40d656626dbb4f1572ed0300093791c9610`
(economy sleep and return routing); that update is outside this comparison.

| Original port | Port on this branch |
|---|---|
| `ed16d5843` mover bitset | `81bff35ef` |
| `7af505d78` changed-ID dirty bits | `10fa8d9e1` |
| `a8dc25398` presence buckets | `56ad0ebe2` |
| `2463007d1` sparse collision grids | `02a0a5eb3` |

The obsolete allowlist commit `aaafd94e4` was not ported. Tests were placed in
test files. The separate observer commit `4a27e6520` keeps clocks in the server
and harness adapters through the simulation's existing `PlanningProbe`.
The observer code is identical on both measured versions. Collision and presence
times measure relocation, excluding spatial queries; they are inclusive within
the outer tick phases and must not be added to movement. Nanoseconds accumulate
before conversion to milliseconds. Observer overhead remains in tick timing.

## Measurements

One run per case, sequentially admitted through the shared long-job slot, on a
shared 12-core host. No sampling profiler ran during these official measurements.
All values below are milliseconds except ticks/s. Each phase cell is p50 / p95.

| Workload / version | Ticks/s | Tick p50 / p95 | Movement p50 / p95 | Collision relocation p50 / p95 | Presence relocation p50 / p95 |
|---|---:|---:|---:|---:|---:|
| Rust bots 1x / before | 18.075 | 19.776 / 51.870 | 14.111 / 31.195 | 0.846 / 1.455 | 0.000 / 0.005 |
| Rust bots 1x / after | 18.807 | 19.460 / 47.951 | 13.892 / 34.372 | 0.832 / 1.349 | 0.000 / 0.004 |
| Rust bots 5x / before | 7.083 | 75.928 / 366.185 | 59.132 / 105.904 | 4.260 / 8.159 | 0.005 / 0.017 |
| Rust bots 5x / after | 7.093 | 78.807 / 373.257 | 60.720 / 103.506 | 4.065 / 7.646 | 0.004 / 0.012 |
| Owner save / before | 10.651 | 77.555 / 165.147 | 45.9920 / 90.4555 | 3.3015 / 8.2850 | 0.0090 / 0.0220 |
| Owner save / after | 11.368 | 72.781 / 157.986 | 41.8775 / 85.1640 | 3.1885 / 7.1805 | 0.0060 / 0.0145 |

The Rust-bot benchmarks use 100 seats, seed 1, six workers and 2,000 ticks.
Each version starts with 9,719 units / 4,716 buildings at 1x and 46,791 units /
22,422 buildings at 5x. All 100 required Stewards are garrisoned; no village
setup failed. Both versions end at 10,173 units / 42 movers / 16 waiting at 1x,
and 46,985 units / 592 movers / 523 waiting at 5x.
Benchmark ticks/s excludes bot decisions, hashing and setup. Including the bot
loop, periodic hashing and report sampling, throughput is 16.613 / 17.287 before / after at 1x,
and 6.265 / 6.268 at 5x. Setup is 71.181 / 74.824 seconds at 1x and
341.200 / 344.924 seconds at 5x, excluded from both throughput measures.

Each benchmark pair has exactly 21 nonempty canonical checkpoints, ticks
0, 100, ... 2,000. Both complete sequences agree, including setup and final
hashes. The `MATCH_BENCH_ARGS='--compare ...'` runs report 21 compared and `all_equal: true`
for each scale. A single-worker-count report's empty internal comparison list
is not the evidence for this before/after equality.

The owner save completed 5,120 ticks in 480.721 seconds before and 5,462 ticks
in 480.478 seconds after. Population was 9,351 to 9,618 before and 9,351 to
9,606 after. Both reports have `dirty: false`, `failure: null` and eight
phase windows. Both servers and bot hosts stopped when their runs returned.

Saved-game movement/collision/presence values are medians of the eight rolling
minute p50s and p95s, not whole-window phase percentiles. Tick percentiles cover
the complete measured window. The live runs use one passive human connection
and 99 connected Python AIs on eight hosts. Their wall-time-driven orders can
differ; the deterministic hash evidence comes from the fixed Rust-bot traces
and regression tests. A displayed 0.000 ms is reporting precision, not zero work.

## Commands and retained evidence

All reports and logs below are ignored local evidence under
`.cache/phase1-index/`; original game assets, saves and recordings are not in Git.
The save copy is `packages/saves/1794176817-first.save`, with SHA-256
`c51582bf7eb4007ac5cdb72ef2ab3a733c7a451532b6b41d6816a2fc9c85c11a`.
The original under `/home/anko/Work/evidence/saves-2026-11-08/` remained read-only.
Page/navigation data use regular copied directories in the private package
directory. Each run restores the same save; resume does not load a resource
journal from a preceding run.

The following commands ran once per version on its clean measured revision,
with `SIDE` replaced by `before` or `after` in the output path and label:

```sh
make match-bench MATCH_BENCH_ARGS='--scale 1 --ticks 2000 --threads 6 --out .cache/phase1-index/c5-SIDE-scale1.json'
make match-bench MATCH_BENCH_ARGS='--scale 5 --ticks 2000 --threads 6 --out .cache/phase1-index/c5-SIDE-scale5.json'
make scale-match SCALE_MATCH_ARGS='--save .cache/phase1-index/packages/saves/1794176817-first.save --package-directory .cache/phase1-index/packages --minutes 8 --port 8175 --no-summary --label phase1-index-c5-SIDE --out .cache/phase1-index/c5-SIDE-save.json'
```

The completed baseline cases ran before the indexed cases. Each Make invocation
released the long-job slot before the next case was admitted. Comparisons:

```sh
make match-bench MATCH_BENCH_ARGS='--compare .cache/phase1-index/c5-before-scale1.json,.cache/phase1-index/c5-after-scale1.json'
make match-bench MATCH_BENCH_ARGS='--compare .cache/phase1-index/c5-before-scale5.json,.cache/phase1-index/c5-after-scale5.json'
```

Matching `.log` files retain command output; `c5-compare-scale{1,5}.log` retains
the comparison verdicts. The saved-game server logs are copied to
`c5-{before,after}-save-server.log`. Sampled host loads are in the corresponding
`*-load.log` files, and `comparison.json` summarizes the checked benchmark data.
The older `before.json` / `before-profile/` run on `cf385ae1b` is preliminary,
heavily loaded evidence and is excluded from the tables above.

## Validation and limits

`make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation
AOE_NATIVE_TEST_FILTER='all()'` passed 1,211 tests with zero skipped, including
movement, presence, collision and the unchanged full-population checkpoint's
one/four-worker comparison. Focused server timing/admin tests passed 14/14;
`make test-harness-focused HARNESS_TEST_FILTER=match_bench::` passed 7/7,
including the small worker replay and phase-sample assertions. New observer
tests first failed for missing spans, lost submicrosecond spans and absent
benchmark phase samples, then passed without changing a world-hash pin.

`make -j1 hooks-install hooks-check fmt-check lint structure-check
architecture-check docs-check` passed before the report commit; its log is
`final-gates.log`. Normal commit hooks also passed. The earlier stop-cadence
architecture timeout was a Cargo build-directory lock wait under host load 70.6.
The isolated `make architecture-check` retry passed (`stop-architecture-retry.log`);
no test, gate budget or baseline was weakened. The initial failing hook state
is retained in `stop-hook-before-final.json`. Simulation/server/harness logs are
`index-timing-green-{simulation,server,harness}.log`; baseline observer checks
are in `baseline-timing-{simulation,server,harness,gates,commit}.log`.

Sampled one-minute host load ranges, from admission through completion
(including build/setup), were:

| Case | Before | After |
|---|---:|---:|
| scale1 | 16.48–30.48 | 14.10–21.87 |
| scale5 | 11.92–17.93 | 10.83–30.90 |
| save | 16.09–29.16 | 12.03–30.03 |

These single runs do not establish a causal index speedup, finished RTS
performance, sustained 20 Hz, or dedicated-hardware qualification. Collision
query cost is not isolated by these relocation spans. CI, browser/e2e QA,
longer replay qualification and performance on the later economy dev update
remain unverified. This storage/observer change has no intended visible game
behavior change; [showcase guidance](../showcase.md) permits performance work
with nothing visible to carry numbers without a video. Nothing was published.

## PR #749 review fixes: F1–F3

The fixes start from `0d5b7845a44eeb6e9fdef847ef00c75f3974d36a` on
`ship/phase1-index`, without rebasing. F1 anchors the presence array and both
collision directories at their first occupied coordinate. Their rectangles grow
geometrically in every direction, with spare capacity on the growing side;
lookups remain direct array reads. Growth preserves bucket order, swap-removal
slots, collision arena slots and sampling order. These are growing rectangles,
not a fully sparse directory: widely separated occupied coordinates and retained
historical bounds can still reserve empty entries between them.

The 16,384 × 16,384 far-corner regression measures the struct plus all its vector
capacities on the 64-bit native target, excluding allocator metadata and RSS:

| Storage / setup | Before (bytes) | After (bytes) |
|---|---:|---:|
| Presence: 32 IDs in chunk (511, 511) | 6,291,624 | 280 |
| Fine collision grid: one set tile (16,383, 16,383) | 4,195,420 | 1,148 |
| Coarse land-count grid: one set cell (8,191, 8,191) | not measured | 1,148 |

The after-byte counts are asserted by the tests. Separate tests exercise 32
far-corner collision bodies across land/naval layers, coarse-count removal,
four-direction growth, page reuse, and successive left/up growth without
repeated doubling.

F2/F3 restores `collect_changed.visited` to its input-ID count, including repeated
reports before deduplication. One mover among 1,024 idle villagers produces
exactly 20 visits over 20 ticks, rather than zero or the whole population.
A separate test verifies repeated/missing IDs, sorted deduplicated output and
empty calls. The existing counter test again requires actual changed work.

The initial four selected regressions all failed on the original implementation
(`.cache/ixfix-red.log`). Focused checks then passed 14/14; the first green attempt
caught a wrong import in the new test, corrected before the passing retry:

```sh
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(far_corner_) | test(collect_changed_visits_) | test(the_tick_passes_count_)'
make fmt
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(chunk_buckets::) | test(collision::index::) | test(scale_hooks::passes::) | test(economy::tick::tests::)'
```

Focused evidence is `.cache/ixfix-green-focused-retry.log`; the failed import
attempt is `.cache/ixfix-green-focused.log`. No hash pin, gate or existing
performance baseline changed. No visible game behavior changes; the
[performance-video exemption](../showcase.md) applies. No footage was recorded,
and no long-job benchmark, publication or CI run was requested for these fixes.

The final full simulation suite passed **1,227/1,227**, zero skipped, in 286.736
seconds, including the unchanged 14,000-unit, 384-tick checkpoint and its
one/four-worker comparison. The earlier run also passed 1,227/1,227 in 409.930
seconds. These are regression runs on a shared machine, not a new throughput
qualification.

```sh
make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'
make -j1 hooks-install hooks-check fmt-check lint structure-check architecture-check docs-check
```

Final full-suite output is `.cache/ixfix-simulation-all-final.log`; the earlier
run is `.cache/ixfix-simulation-all.log`.

The first gate sequence (`.cache/ixfix-gates.log`) passed hooks, formatting, lint
and structure, then architecture rejected two new production `.expect()` calls.
Those calls were removed in favor of offsets within the just-grown bounds,
without changing a gate or adding an allowlist.

The final gate sequence passed all seven targets; output is
`.cache/ixfix-gates-final.log`. Normal commit hooks recheck the staged tree.
CI, browser/e2e QA, real-map startup RSS and timing remain unverified for these
review fixes. The reported allocations do not qualify finished RTS performance
or dedicated hardware.
