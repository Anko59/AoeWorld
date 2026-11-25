# Rust AI changes

`aoe-ai` is the everyday computer player: the Python AI of `ai/aoe_ai` ported to
Rust, module for module (`play/economy.py` is `play/economy.rs`, and so on). It
plays through the SDK's fog-filtered `World` only, never through simulation
state, and it sends ordinary `Order`s. Keep every tunable in `config.rs`, keep
decisions deterministic (ordered maps and sets, no clocks, no randomness that
is not seeded from the world), and port a Python test with each behaviour into
`src/tests/`. Python stays the reference for tournaments: when one side changes
strategy, say in the PR whether the other follows. Run focused tests with
`make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-ai AOE_NATIVE_TEST_FILTER='test(<module>::)'`.
