# Simulation changes

Keep state integer, seeded, and environment independent. Preserve canonical
ordering and spatial-query correctness. Read [engineering](../../docs/agent-engineering.md)
and [testing](../../docs/testing.md); run focused tests with `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation AOE_NATIVE_TEST_FILTER='test(navy::)'`.
