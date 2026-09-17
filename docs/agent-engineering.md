# Agent engineering guide

## Ownership

`aoe-core` owns typed identifiers and coordinates. `aoe-scenario` owns seeded
workload definitions. `aoe-simulation` owns deterministic state and spatial
queries. `aoe-protocol` owns wire messages. `aoe-server` owns configuration,
HTTP, WebSocket lifecycle, and clocks. `aoe-harness` owns policy and commands.
Rendering, assets, QA, and release adapters must consume these boundaries.

Simulation and core code must avoid OS, browser, filesystem, network, and clock
APIs. Keep transport types out of simulation storage. Add compatibility fixtures
for protocol changes and deterministic replay checks for simulation changes.
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
