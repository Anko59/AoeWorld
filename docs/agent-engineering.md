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

Select focused validation from [testing](testing.md); `make preflight` is the
minimum handoff gate. A change in shared types, dependencies, toolchain, CI, or
unknown paths requires all relevant suites. Never accept a baseline merely
because a new result regressed. If nearby maintenance is discovered, make only
small behavior-preserving fixes in the owning module under the same gate;
record other findings separately.

Handoff must state the exact Git revision, dirty-tree status, Make commands and
results, CI evidence for that revision, untested paths, and external
qualification still needed. A missing gate is an explicit limit, not a pass.

## Local live gate triage

`gate-run` retains its legacy local ledger fields with schema 2. That ledger,
its reasons, and its stdout/stderr log files remain **UNSANITIZED LOCAL** data.
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
