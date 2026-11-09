# Phase-1 wire bookkeeping on current dev

Port revision: `304f9f0d6a9f2c01578f7e4b123ae6ba46a41d41`.
Baseline: `04cfd579c8429dce3e1decae539c71ce200068e2` (fetched origin/dev).
The five source changes were applied in order and committed together:
`379c84057`, `26825bced`, `a6e2ecae6`, `bbd9f7a31`, `ac6c06789`.
`494a2a670` was excluded. The delivery conflict preserves dev's forward cursor;
dev's parallel hashing, visibility watcher work and cap ratchet are unchanged.

## Workload and evidence

The owner's saved world `1794176817-first.save` was copied, with its JSON card,
into ignored `.cache/phase1-wire/before-map-store/saves/`. Its SHA-256 is
`c51582bf7eb4007ac5cdb72ef2ab3a733c7a451532b6b41d6816a2fc9c85c11a`.
Original evidence files were never modified. The card describes tick 430300,
99 opponents, 9,395 units and 10,581 buildings on package `dc7e0d4b…`.
Each side uses a private copy of `local-assets/map-packages`, with its save
pair replaced by the evidence copy. No game data is committed.

Each side runs separately, on port 8173, with its own long-job admission:

```sh
make scale-match SCALE_MATCH_ARGS='--save .cache/phase1-wire/before-map-store/saves/1794176817-first.save --package-directory .cache/phase1-wire/before-map-store --minutes 8 --port 8173 --label phase1-before --out .cache/phase1-wire/before.json --no-summary'
```

The after invocation changes `before` to `after` in directory, label and output.
Raw JSON, process/server logs and host-load observations stay under ignored
`.cache/phase1-wire/`. The long-job slot was busy on initial attempts; those
attempts did not run a match. Builds were prepared with `make sdk-python` and
`make dev-build` while waiting, without holding the long-job slot.
One admitted startup failed before any match because symlinked package manifests
are rejected; regular copies corrected the local layout without code changes.

## Measurements

The baseline JSON identifies clean revision `04cfd579c…`, no failure, 100 seats,
99 AI opponents on eight hosts, 2,000 tiles per axis and eight minutes requested.
It contains 3,796 ticks over 480.627 seconds (7.898 ticks/s), with 9,351 units
at the measured start and 9,582 at the end. Its SHA-256 is
`180336ab1a5964bbbde35ebcd72aa8aa8c8a4bf2c3a879b5d91b2aaedc9ee700`.

The after JSON identifies clean port revision `304f9f0d6…` and no failure,
with the same seat/host/map configuration. It contains 5,258 ticks over
480.828 seconds (10.935 ticks/s), with 9,351 units at the measured start
and 9,615 at the end. Its SHA-256 is
`3f1511b975eb91d7de4f48aba4b937f27d2f17326076860120db8c0a11d39dbe`.
Both Make invocations exited successfully, with no failure reported by the
runner's saved-skirmish readiness or human-socket checks. The runner stopped
its server and bot containers.

Phase figures are milliseconds: the median of the eight minute-end rolling
`/admin/perf` p50 values and, separately, the median of its p95 values. These
are not recomputed whole-run phase percentiles. `frame` means the server's
shared unit frame, not a browser render frame. Tick rate is whole-run ticks
divided by measured wall time.

| Server phase (p50 / p95 ms) | Before | After |
|---|---:|---:|
| fanout_wait | 8.633 / 49.557 | 7.247 / 26.550 |
| frame | 3.217 / 8.402 | 2.677 / 5.580 |
| session_views | 4.545 / 10.387 | 2.994 / 6.721 |
| viewers | 0.189 / 10.743 | 0.426 / 7.569 |
| delivery | 1.953 / 9.184 | 1.072 / 5.438 |
| ticks/s (whole run) | 7.898 | 10.935 |

Viewers p50 increased. The lower wall times and higher tick rate cannot be
attributed solely to this port under these different contention conditions.

The cumulative counters between the first minute observation and the final
observation provide counts independently of wall timing:

| Counter delta (minute 1 to end) | Before | After |
|---|---:|---:|
| unit frames built | 3,607 | 4,935 |
| units visited | 34,176,930 | 46,949,181 |
| units converted afresh | not separately instrumented | 14,529,230 |
| units marked changed | not separately instrumented | 8,630,421 |
| movement table refresh calls | 7,214 | 9,870 |
| movement table entries visited | 743,352,022 | 19,536,004 |

After, 30.95% of visited units were converted afresh, and 18.38% were marked
changed. Movement table entries visited averaged 103,043 per refresh before
and 1,979 after. The latter counter deliberately changes from all allocated
entity IDs to the active job IDs marked by the sparse refresh; it describes
the work performed, not a claim that both runs executed the same tick/order log.
This counter records current marks; clearing previous marks and vector growth
are not separately counted. It is not a complete memory-access count.

The baseline's one-minute host load average was 88.92 before startup and 61.34
after completion on the shared 12-core machine; after's was 11.43 and 23.72.
This is not a quiet-machine causal comparison: host contention varies, and AI
play can diverge after the same restored start. Neither run demonstrates 20 Hz
or qualifies dedicated hardware. Server CPU averaged 0.758 / 0.998 cores and
peaked RSS was 419 / 436 MiB before / after. Missed tick rates were 99.3% / 87.9%.

`origin/dev` advanced to `cf385ae1b` during the work. The comparison and port
remain anchored to the explicitly fetched `04cfd579c…` baseline; integration
with subsequent dev changes is unverified.

## Validation

All commands used Dockerized Make targets on the port's code:

- `make hooks-install hooks-check`: passed.
- `make fmt-check lint structure-check architecture-check`: passed.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server AOE_NATIVE_TEST_FILTER='test(gameplay)|test(routes)'`:
  123 passed, including `per_session_messages_match_the_pinned_stream`,
  `tick::dirty` and the conversion-counter regression. Wire pins are unchanged.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'`:
  1,207 passed, none skipped, including fog equality and replay/hash regressions.
  Nextest reported two leaky passing tests: combat corpse expiry and secondary
  castle-arrow hills coverage; the command exited successfully.
- `make test-harness-focused HARNESS_TEST_FILTER=perf::`: nine passed.
- The normal commit hook's exported-index pre-commit checks passed, including
  native Clippy and WASM test compilation. Lint reports two existing client
  dead-code warnings (`travel_over`, `travel_at`).

The evidence-only follow-up runs `make hooks-install hooks-check fmt-check lint
structure-check architecture-check docs-check` before its normal hooked commit.
No native suites are repeated for this Markdown-only change.

No client files changed. Full workspace suites/CI, browser/video behavior,
dedicated hardware and finished RTS performance are unverified. No publication
was run, as requested by the task brief. This is a shared-machine saved-game
measurement, not evidence for every RTS workload. There is no visible behavior
change to film; unchanged wire pins are the behavior regression evidence.
