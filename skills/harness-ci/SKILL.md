# Harness and CI

Read [engineering](../../docs/agent-engineering.md), [gates](../../docs/gates.md), and `crates/harness/AGENTS.md`. Put policy and orchestration in tested Rust code. Keep Make and workflows as dispatchers. Run `make test-harness-focused HARNESS_TEST_FILTER=agents::` or another relevant filter, and `make browser-check` when browser behavior changes. The agent-runtime judge (`crates/harness/src/agents/`) is described in [agent runtimes](../../docs/agent-runtimes.md); add a policy test for every rule it gains.
