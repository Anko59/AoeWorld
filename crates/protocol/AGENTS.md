# Protocol changes

Maintain bounded versioned messages with one current version (no backward
compatibility before v1.0) and golden fixtures that pin the current wire. Read
[transport ADR](../../docs/adr/0002-transport.md) and run focused tests with `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-protocol AOE_NATIVE_TEST_FILTER='test(messages_round_trip)'`.
