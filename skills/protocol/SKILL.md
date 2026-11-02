# Protocol

Read [transport ADR](../../docs/adr/0002-transport.md) and `crates/protocol/AGENTS.md`. Preserve versioned bounded decoding and golden fixtures that pin the single current version (no backward compatibility before v1.0; other versions are rejected). Run focused tests such as `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-protocol AOE_NATIVE_TEST_FILTER='test(<changed_module>::)'`; include malformed, stale-version, and reconnect cases for wire changes. Full e2e runs in CI.
