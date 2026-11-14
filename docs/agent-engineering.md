# Agent engineering guide

## Ownership

`aoe-core` owns typed identifiers and coordinates. `aoe-scenario` owns seeded
workload definitions. `aoe-simulation` owns deterministic state and spatial
queries. `aoe-protocol` owns wire messages. `aoe-server` owns configuration,
HTTP, WebSocket lifecycle, and clocks. `aoe-harness` owns policy and commands.
Rendering, assets, QA, and release adapters must consume these boundaries.

Simulation and core code must avoid OS, browser, filesystem, network, and clock
APIs. Keep transport types out of simulation storage. Add golden fixtures
pinning the current wire format for protocol changes and determinism checks
(the same input gives the same output) for simulation changes. When behaviour
changes on purpose, regenerate the affected pins and justify the change in the
PR; a changed replay is never a blocker by itself ([working agreement](../AGENTS.md)).
Performance-sensitive changes require scenario-specific counts and benchmarks,
measured against the scale ladder of #748.

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

## Whole suites need a lease

CI runs whole suites on every pull request, so agents and launched sessions (a
Codex, pi or DeepSeek session started with `AOE_COORDINATOR`) run focused checks:
`test-native-focused`, `test-harness-focused`, `test-wasm-focused`, and `test-e2e`
with 1 to 5 `AOE_E2E_SPECS`. The agent hook refuses whole suites and sweeps
(`preflight`, `test-unit`, `test-harness`, `coverage`, fuzz, and the perf soaks;
the list is in `crates/harness/src/agents/rules/budget.rs`) unless a coordinator
lends one with `make lease AOE_LEASE_TARGET=<suite>` (60 minutes by default, at
most 240).

## Resource admission and long jobs

Every build container takes one of four build slots (`AOE_CPU_SLOTS`; at most 2 per worktree and 3 per
`AOE_COORDINATOR`); it waits up to 90 s, then refuses with exit 75. On that refusal, do non-build
work instead of retrying in a loop. Long jobs also share three slots (`AOE_LONG_SLOTS`, 1..=8). Workload containers
share the memory/swap caps and CPU shares in `gates/container.args`; CPU quotas
are not used. A host MemAvailable check refuses immediately below 3 GiB.
`make load-status` shows available memory and every long-job slot with its holder.

The `long` registry flag covers full e2e, coverage, local perf benchmarks and
aggregates containing them. A nonempty `AOE_E2E_SPECS` selection is short.
The std-only resource runner takes the first free long-job slot (`long-N.lock`) through Linux `flock` before
bootstrapping the harness or starting Cargo. The runner builds its tool image
before compiling itself on a clean checkout. When every slot is held, an invocation names all holders
and stops with `retry later`. Intended children inherit the live descriptor and
validated proof, so nested targets do not refuse themselves and controller death
does not admit a competing long job while its child is running. Do useful work
and rerun later; never poll or retry in a loop. Ship separately locks its rebase
and push, releasing during gates and review; a busy reacquisition stops while
preserving evidence for the commit. See [gates](gates.md) and [shipping](shipping.md).

A new field in `gates/registry.json` ships in two changes: first the code that
reads it (the judge built from `dev` parses the file strictly and refuses an
unknown field), then the file that uses it.
