# Agent engineering guide

## Ownership

`aoe-core` owns typed identifiers and coordinates. `aoe-scenario` owns seeded
workload definitions. `aoe-simulation` owns deterministic state and spatial
queries. `aoe-protocol` owns wire messages. `aoe-server` owns configuration,
HTTP, WebSocket lifecycle, and clocks. `aoe-harness` owns policy and commands.
Rendering, assets, QA, and release adapters must consume these boundaries.

Simulation and core code must avoid OS, browser, filesystem, network, and clock
APIs. Keep transport types out of simulation storage. Add golden fixtures
pinning the current wire format for protocol changes and deterministic replay
checks for simulation changes.
Performance-sensitive changes require scenario-specific counts and benchmarks.

## Workflow

Before each commit run `make fmt-check`, `make lint`, `make structure-check`,
and focused tests for each touched crate (for example `make test-native-focused
AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(navy::)'`
or `make test-harness-focused HARNESS_TEST_FILTER=agents::`). Client changes
also require `make perf-wasm-cap`. See [testing](testing.md). Full suites,
e2e and performance benchmarks run in CI. A change in shared types, dependencies, toolchain, CI, or
unknown paths requires all relevant suites. Never accept a baseline merely
because a new result regressed. If nearby maintenance is discovered, make only
small behavior-preserving fixes in the owning module under the same gate;
record other findings separately.

Handoff must state the exact Git revision, dirty-tree status, Make commands and
results, CI evidence for that revision, untested paths, and external
qualification still needed. A missing gate is an explicit limit, not a pass.

## Local live gate triage

`gate-run` writes its local ledger (schema 2) fresh from this run, never
reading an old one; that ledger, its reasons, and its stdout/stderr log files
remain **UNSANITIZED LOCAL** data.
Console JSON is a separate generated whitelist: actual fixed-Make typed exit
(including numeric Make exit, not recipe exit), bounded raw-tail lengths/BLAKE3,
measured duration, log-retention outcome, original-source/private endpoint
phases, and final-publication outcome. Publication failure emits those current
facts and returns failure; no old ledger is read as a fallback. Missing success
logs remain unavailable, and later success never replaces prior command failure.

Null triage means unobserved, not measured success; synthetic runtime fixtures
must not fabricate observations. Cache-path preconditions have no captured
metrics. Root cause is always not assessed. Snapshot revision/tree/kind are
subject metadata, not clean-working-source, history, provenance, or approval.
Index/Commit scope does not qualify unstaged working inputs. Endpoint checks
cannot establish absence of reverted edits or after-return races.

Mode 0600, sync, and rename are local hygiene, not hostile same-UID isolation or
an assurance that old hostile PASS bytes were erased. Neither capture metadata,
hashes, modes, coding/Docker UID, nor fixture mocks authenticate a restricted
worker, controller, publisher, hidden verdict, approval, runtime QA, or served
build. Qualification and authorization remain independent required planes.

## Resource admission and long jobs

Short gates start without an exclusive lock or load wait. Workload containers
share the memory/swap caps and CPU shares in `gates/container.args`; CPU quotas
are not used. A host MemAvailable check refuses immediately below 3 GiB.
`make load-status` shows available memory and the live long-job holder.

The `long` registry flag covers full e2e, coverage, local perf benchmarks and
aggregates containing them. A nonempty `AOE_E2E_SPECS` selection is short.
Long jobs take one nonblocking kernel lock; a second invocation names its holder
and stops with `retry later`. Intended children inherit the live descriptor and
validated proof, so nested targets do not refuse themselves and controller death
does not admit a competing long job while its child is running. Do useful work
and rerun later; never poll or retry in a loop. Ship separately locks its rebase
and push, releasing during gates and review; a busy reacquisition stops while
preserving evidence for the commit. See [gates](gates.md) and [shipping](shipping.md).

A new field in `gates/registry.json` ships in two changes: first the code that
reads it (the judge built from `dev` parses the file strictly and refuses an
unknown field), then the file that uses it.
