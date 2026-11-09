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

## F1: retain the unit frame and consume simulation changes

The initial failing regression ran against `f2fe06ea79f7` production code:
three idle service ticks visited 12 unit ids rather than the four ids required
by the initial build.

Unit storage now journals mutable access, insertion and swap-removal. The
report maps journal gather/hunt/cargo/army/controller changes at their source.
Member changes also invalidate their General's squad report; Steward territory
events invalidate their workers and fishing ships. These derived journals are
neither hashed nor saved and do not change simulation advancement's output.
Marks are conservative: a mutable access can leave the final wire state equal.

Fan-out takes ownership of the previous frame and updates only the published
change ids. States, indexes and other owner-list entries remain in place.
Owner lists maintain their storage-order cap and id-order output through
ordered selected/overflow sets, including when removal swaps a storage slot.
New viewers use the retained owner index. A missing previous frame, a shared
previous frame that cannot transfer ownership, or a skipped simulation tick
requires a fresh full build. The visit counter counts unique input ids,
including removed ids, rather than every lookup or memory access in the build.

In tests every fan-out independently scans and converts the full world and
checks its states, indexes and capped owner lists. The existing 240-tick dirty
client oracle additionally checks that applying delivered deltas reconstructs
the full view. Focused regressions cover idle ticks, one mover among thousands,
report-only invalidation, storage swaps, death/removal, list overflow and a
missed frame. The wire-stream pin is retained without regeneration.

The in-process comparison uses `Seed(7)`, a 1,024-square world, 13,400 Scouts
on a four-tile grid and 100 owners. All units use `NoAttack`; each measured
simulation advance must return no changed units and publish an empty journal.
This prevents autonomous combat or spawn spacing from turning the idle fixture
into an active workload. An earlier random-population trial did start combat;
its timing is excluded from this idle comparison.

The exact `f2fe06ea79f7` frame source (SHA-256
`0175c45877f9a3e83067bbdd190e7a57705437463c23a621c8bd84508273b6d6`)
was temporarily compiled as a test-only module to capture the scan baseline
alongside a retained frame reading the same world. The final retained frame is
measured again by the focused test against the identical fixture. Only the
current implementation is retained in the committed code. Each case has two
setup ticks and an initial frame before 300 measured builds. Simulation
advancement, including journal publication, is outside the timed spans. Both
builds use one worker in the optimized test profile with assertions enabled.
The nearest-rank p50/p95 exclude sessions, transport and the oracle.

| Steady idle frame metric | Original scan | Retained frame |
|---|---:|---:|
| Unit ids visited, 300 builds | 4,020,000 | 0 |
| Unit ids visited per build | 13,400 | 0 |
| Frame build p50 (ms) | 1.607064 | 0.001467 |
| Frame build p95 (ms) | 2.170898 | 0.001956 |

The one-mover regression visits exactly one id among 13,400 units and retains
an unaffected owner's list. Initial frames still visit the whole population.
The table uses `.cache/phase1-wire/frame-before-idle.txt` and
`frame-final-idle.txt`. The original paired retained measurement is also kept
as `frame-after-idle.txt`: zero visits and p50/p95 0.001885/0.002374 ms.
`.cache/f1-idle-comparison.log` records that paired run; the final measurement
ran in the full server selection below. All runs share the host with other
work; counts establish scan removal independently of scheduling variation.

This is a synthetic idle bookkeeping measurement on a shared host, not a
saved-game rerun, an active-heavy performance comparison, finished RTS
performance or dedicated hardware qualification. Browser/video and CI
qualification remain separate; there is no intended visible behavior change.
No publication is run under this brief's instructions.


### F1 validation

Dockerized Make commands on the final source:

- `make hooks-install hooks-check`: passed.
- `make hooks-check fmt-check lint structure-check architecture-check docs-check`:
  passed. Lint retains the two existing client dead-code warnings.
- `NEXTEST_TEST_THREADS=2 make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server AOE_NATIVE_TEST_FILTER='test(gameplay)|test(routes)'`:
  128 passed, including the unchanged wire-stream pin, `tick::dirty`, the
  full-scan oracle and all new frame regressions.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'`:
  1,210 passed, zero skipped, including one/four-worker replay equality and
  the new mutation-journal regressions.

One earlier default-concurrency server selection passed 127 tests and timed out
waiting for skirmish startup to reach `ConnectingAi` after placing all villages.
The full selection subsequently passed twice with two concurrent tests, with
its tests and timeouts unchanged. The cause of that earlier timeout is
unverified. Logs remain in `.cache/f1-server-verified.log`,
`.cache/f1-server-final-check.log`, `.cache/f1-simulation-verified.log` and
`.cache/f1-final-checks.log`.

## PR #744: simulation instruction regression

On 2026-11-09, `make perf-instructions` ran before any production edits at
`b5bdaaee6aa461a2b28cdfbfe3aaaa290c39fe29`, then at the current `origin/dev`,
`8ca8d40d656626dbb4f1572ed0300093791c9610`. Both ran serially in this worktree
with the pinned Gungraun 0.19.4 / Callgrind 3.19.0 analysis image. The branch
was restored without rebasing. Dev includes #746's economy changes, which are
not imported by this fix; its counts are a measured comparison, not the gate's
committed baseline.

| Simulation kernel | Committed baseline | Current dev | Broken F1 | Fixed F1 |
|---|---:|---:|---:|---:|
| distributed_ticks | 10,330,744 | 8,983,949 | 27,705,150 | 8,943,073 |
| smoke_ticks | 5,275,671 | 5,095,419 | 6,726,281 | 5,224,565 |
| late_game | 240,557,748 | 192,013,384 | 220,668,282 | 207,084,754 |

The distributed per-function profiles identify the journal as the regression:
`publish_unit_changes` costs 16,224,232 inclusive instructions (58.6% of F1's
total), and is absent on dev. Its conversion of the ordered set to a vector
costs 10,419,522 inclusive instructions; its General-dependency collection costs
5,480,908. These two child costs are included in publication, not additional
costs to add to it. Ordered-set destruction alone has 6,519,127 self
instructions, and ordered-set iteration has 4,444,444. `apply_step` also spends
1,411,035 inclusive instructions in 4,096 ordered-set insertions.

The four-tick population fixture starts immediately after spawning 128,000
units. Only 512 are ordered to move, but the first publication consumes all
128,000 spawn marks. This explains the large initial publication cost without
claiming that every unit moves every tick.

Unit mutation marks now use a bitmap to deduplicate a sparse id vector.
Publication swaps reusable vectors, sorts only out-of-order insertions, and
clears only touched bitmap words. With no controlled members it skips General
lookups entirely. Report journals, mutation coverage and dependency semantics
are retained, as are the movement-table and fog improvements. No baseline,
allowance, workload or wire pin changes.

The fixed distributed profile reduces `publish_unit_changes` from 16,224,232
to 231,922 inclusive instructions. Fixed distributed, smoke and late-game
counts are respectively 13.4%, 1.0% and 13.9% below their committed baselines.
The retained frame's late-game gain is preserved and improved.

Raw NDJSON and Callgrind profiles are retained outside Git in
`.cache/perf744/{head,dev,fixed}-simulation.ndjson` and
`.cache/perf744/{head,dev,fixed}-profiles/`. `make perf-groups-annotate` with
`GROUP_CALLGRIND` pointing at each distributed profile and
`GROUP_CALLGRIND_INCLUSIVE=yes` or `no` produced the inclusive/self reports in
the same evidence directory.

The sparse-id regression was added and passed before changing production code;
the existing instruction measurements above supply the failing performance
regression. It checks repeated, out-of-order writes across ids 63, 64 and 4097,
overlapping report invalidation, whole-store mutation and repeated idle clears.

Validation of the replacement, through Dockerized Make targets:

- `make fmt`: passed.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'`:
  1,211 passed, zero skipped, including one/four-worker replay equality.
- `NEXTEST_TEST_THREADS=2 make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server AOE_NATIVE_TEST_FILTER='test(gameplay)|test(routes)'`:
  128 passed, including the unchanged `per_session_messages_match_the_pinned_stream`,
  `tick::dirty` and the test-only full-frame oracle on every fan-out.
- The idle frame test still records zero visited units across 300 builds with
  13,400 units; p50/p95 are 0.001467/0.001955 ms on this shared host. The
  one-mover and spawn/death/storage-swap oracle regressions also pass.
- `make perf-ci`: passed, including its fresh `make perf-instructions` run,
  all seven instruction comparisons at the unchanged 3% cap, WASM size and
  smoke/distributed/hotspot protocol workloads. All three late-game repetitions
  completed. Improved-count notes request a baseline refresh; none is made.
- `make hooks-install hooks-check fmt-check lint structure-check architecture-check docs-check`:
  passed. Lint retains the two existing client dead-code warnings
  (`travel_over`, `travel_at`). The final evidence-text update also passes
  `make fmt-check structure-check docs-check`.

Logs are `.cache/perf744/{journal-before,simulation-final,server-final}.log`
and `.cache/perf744/frame-final.txt`. An initial test invocation was cancelled
while waiting for the formatter's Cargo lock; the completed simulation run
above started after formatting finished.

`.cache/perf744/perf-ci-final.log`, `ci-final.json` and `ci-final.md` retain
the complete local aggregate verdict. It measured the replacement before
commit (`dirty: true`); `fixed-source.sha256` pins its two changed Rust files.
Static gate output is `.cache/perf744/static-final.log`.

No client files changed. There is no intended visible behavior change to film;
unchanged wire pins and the independent full-frame oracle are the behavior
regression evidence. No video is recorded. Browser execution, dedicated
hardware, saved-game timing and finished RTS qualification remain unverified.
Publication and CI for the new commit are not run: the brief explicitly forbids
ship/push. Recordings and original assets remain outside Git.

## Rebase of #744 onto #747

The rebase is complete onto `55f6357742b163a22b34ee9331ed612140440259`.
The merged simulation/server source is committed as
`c5bac8b58826e77b6cefa531462d89c5092717f8`, following the F1 resolution
`9db881a51`. The earlier port and evidence commits were replayed too.
The final `0aedb7a9c` line-count patch became empty because its exact comment
change was included in the F1 resolution before running the structure gate;
`economy.rs` is 500 lines.

`GameWorld` uses `changes::Units` in both configurations. Production embeds a
`Vec<StoredUnit>`; tests embed #747's atomic access-counting `UnitStore` under
that journal. Indexed and bulk access still passes through the probe. Explicit
`len`/`is_empty` methods expose no slots. Mutable scalar access marks its unit,
and mutable iteration marks all exposed ids. Test-only mutable slice/range
helpers also journal their exposed ids; production has no `DerefMut` escape.
Internal journal reads avoid charging the same exposed slots twice. The
unmeasured collision correctness oracle remains separate from the probe.
The optimized sparse bitmap journal and reusable publication vectors survive
the second conflict without losing the test probe.

The initial focused test build failed on the missing merged probe/view APIs.
After the resolution, all ten focused journal/probe tests passed. A temporary
read-only full loop injected into `collision_closed` then made
`collision_closed_accesses_only_subscribed_units` fail with **104 unit accesses**
against its unchanged limit of 16. The injected loop was removed, the focused
selection passed again, and no experimental scan is committed. A new regression
checks that bulk mutable views publish their ids while read-only views and
subsequent idle ticks leave the journal empty.

Validation of the final source, using Dockerized Make targets:

- `make hooks-install hooks-check`: passed.
- `make fmt-check lint structure-check architecture-check docs-check`: passed.
  Lint reports only the existing client `travel_over`/`travel_at` warnings.
- `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='all()'`:
  1,229 passed, zero skipped, including the full late-game replay and sleep
  replays, one/four-worker equality, collision probes and journal regressions.
- `NEXTEST_TEST_THREADS=2 make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server AOE_NATIVE_TEST_FILTER='test(gameplay)|test(routes)'`:
  128 passed, 319 outside the selection. The unchanged wire-stream pin,
  `tick::dirty`, full-frame oracle, idle zero-visit, one-mover and storage-swap
  regressions all passed. The idle measurement visits zero ids over 300 frames
  with 13,400 units.

Logs remain outside Git in `.cache/wrmerge-{red,green,injected-scan}.log`,
`.cache/wrmerge-f1-{gates,simulation,server}.log`,
`.cache/wrmerge-final-{gates,simulation,server}.log` and
`.cache/wrmerge-continue{1,2}.log`. The first `make perf-ci` was refused by a live
shared saved-match job (error 75); `.cache/wrmerge-perf-admission.log` records
that refusal, which is not a performance result.

### Post-rebase performance result

The single later retry of `make perf-ci` was admitted and passed at clean
revision `e5adcf109a00302cae593f2feddb547e25d687ea` (`dirty: false`). Its Rust
source is identical to `c5bac8b58826e77b6cefa531462d89c5092717f8`; the intervening
commit records the rebase/native evidence above. The aggregate freshly ran
instruction benchmarks and WASM size, completed all three late-game repetitions,
and passed the smoke, distributed and hotspot protocol workloads.

| Simulation kernel | Pre-rebase reference | Rebased measurement | Unchanged baseline |
|---|---:|---:|---:|
| distributed_ticks | 8,943,073 | 8,950,261 | 10,330,744 |
| smoke_ticks | 5,224,565 | 5,238,605 | 5,275,671 |
| late_game | 207,084,754 | 196,808,435 | 240,557,748 |

The reference column is the earlier branch measurement recorded above, not a
new paired run. All three rebased counts are below their committed baselines.
All seven instruction comparisons passed the unchanged +3% allowance. Optimized
gzip WASM measured 396,211 bytes against baseline 375,952 with its unchanged 25%
allowance; all eight aggregate comparisons passed. Improved-count notes request
a baseline refresh; no baseline, threshold, workload or wire pin was changed.
The standalone `make perf-wasm-size` also passed while admission was busy.

The fresh aggregate JSON/Markdown, raw instruction NDJSON and Callgrind profiles
are retained outside Git as `.cache/wrmerge-ci.{json,md}`,
`.cache/wrmerge-{simulation,protocol,assets}.ndjson` and
`.cache/wrmerge-callgrind/`. Console output is `.cache/wrmerge-perf-ci.log`;
`.cache/wrmerge-wasm.log` records the standalone size measurement.

The final documentation follow-up changes no Rust source. No client files
changed, and there is no intended visible behavior change to film. No video is
recorded; the unchanged wire pin and independent full-frame oracle are the
behavior regressions. CI, browser execution, a new saved-game timing comparison,
real-source and dedicated hardware qualification, and finished RTS performance
remain unverified. No ship, push or PR publication was run, as the brief requires.
