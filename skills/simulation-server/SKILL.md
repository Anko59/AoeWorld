# Simulation and server

Read [architecture](../../docs/architecture.md), [engineering](../../docs/agent-engineering.md), and the scoped instructions in `crates/simulation/AGENTS.md` and `crates/server/AGENTS.md`. Keep simulation deterministic and independent of transport or wall-clock APIs. Run `make preflight`, the server integration tests, and `make perf-ci` when update or query costs change. Report scenario counts and revision.
