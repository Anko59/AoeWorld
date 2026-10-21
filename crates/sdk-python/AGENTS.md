# Python bindings changes

`aoeworld` mirrors the Rust SDK for Python: keep it a thin layer (types,
conversions, GIL handling) and put behaviour in `crates/sdk`. Every native
name needs a `.pyi` stub (`tests/test_stubs.py` compares them). Waits must
release the GIL. The crate stays outside the Cargo workspace; build and test
with `make sdk-python` and `make sdk-python-test`. Read
[the AI SDK guide](../../docs/ai-sdk.md).
