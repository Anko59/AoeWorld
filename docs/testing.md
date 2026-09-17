# Testing

Use `make fmt-check`, `make structure-check`, `make lint`, and `make test-unit`
for focused checks. `make pre-commit` runs static gates; `make preflight` adds
native tests. The repository must not count compilation as browser execution.
Browser, WASM, asset, performance, QA, and artifact gates need separate evidence.
Do not claim a complete gate until they exist and pass at the exact revision.
