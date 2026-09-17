# AoeWorld Harness Lab

This repository develops a synthetic Rust server/client workload before RTS gameplay.
The current application moves seeded entities deterministically; it has no
pathfinding, combat, economy, or fog of war.

On Linux x86-64, install Git, Make, Docker, and Docker Compose, then run
`make bootstrap`, `make preflight`, and `make dev`. The server listens on
`http://localhost:8080/health` and accepts versioned binary WebSocket messages
at `/ws`. See [documentation](docs/index.md) for component ownership, validation,
and known limits.

Original code is MIT licensed. See [third-party notices](THIRD_PARTY.md) for
asset boundaries.
