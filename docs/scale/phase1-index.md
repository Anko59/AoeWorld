# Phase-1 index port: measurement record

This work ports the mover bitset, changed-ID dirty bits, flat presence buckets
and sparse collision grids. No replay pin or gate manifest was changed.
Original saves, copied assets, raw measurements and profiles remain outside Git.

## Preliminary saved-game run

This is preliminary evidence at `cf385ae1b6e5b300f643a5015bed2fc09837bb67`,
before the benchmark-tool commit landed. It is not the completed comparison
against the newer dev revision `c5f3f0afa34d1a1e0ef8d177c7e48a1dbecbb077`.

The read-only owner save was copied into `.cache/phase1-index/`; its SHA-256 is
`c51582bf7eb4007ac5cdb72ef2ab3a733c7a451532b6b41d6816a2fc9c85c11a`.
The server required regular copied page/navigation directories, rather than
symlinks. The original evidence files were not modified.

`make scale-match` used the copied save, 8 minutes, port 8175, `--no-summary`,
and `--profile .cache/phase1-index/before-profile --profile-after 300
--profile-seconds 60`. The report is `.cache/phase1-index/before.json`.

| Observation | Preliminary baseline |
|---|---:|
| Reported revision dirty | false |
| Failure | none |
| Wall seconds / measured ticks | 482.387 / 3,570 |
| Ticks/s | 7.401 |
| Tick p50 / p95 (ms) | 108.257 / 263.642 |
| Movement p50 / p95 (ms) | 50.310 / 132.741 |
| Units first / last | 9,351 / 9,575 |

Movement values are medians of the eight per-minute rolling phase percentiles,
not percentiles over all ticks. Tick values use the complete measured window.
There were 100 seats, one passive human connection and 99 connected Python AIs.

The host was heavily shared: one-minute load reached about 94 during the run
on a 12-core machine. The sampler collected 13,301 process-CPU samples during
60 seconds; its stacks do not identify the index-relocation stages separately.
Missing stage samples do not establish zero cost. These observations do not
establish an index speedup, 20 Hz performance or dedicated-hardware qualification.

## Validation and remaining work

The four original ports passed `make hooks-install`, `make hooks-check`,
`make fmt-check`, `make lint`, `make structure-check`, `make architecture-check`
and focused simulation tests before each commit. The complete
`make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation
AOE_NATIVE_TEST_FILTER='all()'` selection passed 1,209 tests before the
clearance-page dev update. The unchanged 14,000-unit checkpoint also passed on
`cf385ae1b`, including its one/four-worker comparison.

After the clearance-page update, all static checks and all 1,210 simulation
tests passed at `8212605365d430ec635b7555ceb650f0a03c0ecd` (zero skipped),
including the unchanged full-population checkpoint. The benchmark target was
unavailable during the earlier attempts; it has now landed in `c5f3f0afa`.
Remaining work is to integrate that dev revision, measure its
saved-game and 1x/5x benchmark baselines against the index branch, compare their
hash sequences, and obtain separately identified collision/presence relocation
timings through the existing observer. CI, browser/video QA and hardware
qualification have not been run for this port. Nothing has been published.
