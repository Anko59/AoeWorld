# Server changes

Keep environment reads in `config.rs`, bound client work, and clean up failed
connections. Read [architecture](../../docs/architecture.md) and run focused
integration coverage with `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server AOE_NATIVE_TEST_FILTER='test(<changed_module>::)'`.
