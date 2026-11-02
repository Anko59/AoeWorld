# Simulation and server

Read [architecture](../../docs/architecture.md), [engineering](../../docs/agent-engineering.md), and the scoped instructions in `crates/simulation/AGENTS.md` and `crates/server/AGENTS.md`. Keep simulation deterministic and independent of transport or wall-clock APIs. Run focused tests such as `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(navy::)'` and relevant server integration tests. Performance benchmarks run in CI. Report scenario counts and revision.
