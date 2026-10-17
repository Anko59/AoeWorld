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

## Heavy gates queue

Gates that run the workspace's tests or heavy builds (`test-unit`,
`test-wasm`, `test-e2e`, coverage, mutation, perf; the `heavy` flag in
`gates/registry.json`) take one of two machine-wide slots and queue first come,
first served (static checks such as `lint` never queue: they run in the Stop
hook), so many agents on one machine no longer push each other past their
deadlines ([gates](gates.md)). Do not wait for the load to fall by hand, and
never scan processes to decide when to start: run the target and let it queue.
Before starting heavy work it also waits up to 30 minutes while the 1-minute
load exceeds the core count, reporting every 5 minutes. Its deadline starts
when it runs. SIGINT/SIGTERM stops a load wait and releases the slot. Load is
checked once at admission; nested stages and supervised children inherit the
validated slot token and queue directory. Only a validated inherited slot skips
that wait; a child acquiring its own slot checks load. Integration tests inherit
the parent's slot or hold an admitted fixture ticket in a private queue. Load rising
after start never pauses or aborts running gates. Direct `test-harness`, `test-unit` and `test-wasm` recipes take
the slot before compiling their test steps. So do the make targets that compile or test without
being gates (`build`, `map-test`, `geodata-test`, `test-harness-focused`, `test-placement-coverage`,
`test-geographic-visuals-unit`, `duel-check`, `dev-build`, `test-country-source`): each runs
`$(SLOT_RUN) <target> -- make <target>-steps`, and `aoe-harness slot` queues it whatever the registry
says. Each slot's containers take `cores / slots` CPUs (2 slots in `gates/heavy.json` on 12 cores, so
6 CPUs each): Docker gets `--cpus` and cargo the same job count (`CARGO_BUILD_JOBS`), and the wrapper
container is capped too. Cores are counted from the process's CPU affinity list, not from
`available_parallelism`, so a container's quota does not shrink the machine that the load check sees.
The command-only bootstrap before
admission is static work, serialized across worktrees by flock, with hard
limits of one CPU, one Cargo job and 2 GiB including swap; it builds no tests.
This bounded bootstrap permits cold starts without letting gate compilation
bypass slots. External Cargo/target cache paths and TMPDIR reach both wrapper
and steps containers. `make load-status` shows the holders; a
queue that outlasts `AOE_HEAVY_WAIT_MINUTES` fails with the holders named.

A new field in `gates/registry.json` ships in two changes: first the code that
reads it (the judge built from `dev` parses the file strictly and refuses an
unknown field), then the file that uses it.
