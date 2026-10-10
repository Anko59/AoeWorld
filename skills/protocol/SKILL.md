# Protocol

Read [transport ADR](../../docs/adr/0002-transport.md) and `crates/protocol/AGENTS.md`. Preserve versioned bounded decoding and golden fixtures that pin the single current version (no backward compatibility before v1.0; other versions are rejected). Run `make preflight` and `make test-e2e`; include malformed, stale-version, and reconnect cases for wire changes.
