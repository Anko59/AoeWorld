# AoeWorld

A browser RTS project built with Rust, WASM, and WebGPU. The first playable
feature is one cavalry unit on a grass map using local Age of Empires II
assets. Click or tap to move; click again to redirect. With the map focused,
use arrows/WASD. Reset returns the unit to its starting position.

## Play locally

On Linux x86-64, install Git, Make, Docker, and Docker Compose, then run
`make bootstrap`. Prepare the downloaded trial files and run `make assets-import`
as described in [asset setup](docs/assets.md). Start with:

```sh
AOE_ASSET_PACK=local-assets/packs/<pack-hash> make dev
```

Open [AoeWorld](http://localhost:8080/) in your browser. WebGPU is preferred, with an automatic
Canvas 2D compatibility renderer when WebGPU is unavailable. The server stays running until `make down`. Restart with the pack
setting if a server was already running. Original assets remain local and are
never included in Git or Docker images.

Movement is a deterministic fixed-step simulation running locally in WASM.
This first feature has no multiplayer, persistence, combat, economy, or obstacle
pathfinding. Refresh resets the map. Border trees are scenery.

## Development

Run `make preflight`, `make test-wasm`, and `make test-e2e` for validation.
The browser tests use generated fixtures when no local pack is configured;
`AOE_ASSET_PACK=local-assets/packs/<pack-hash> make test-e2e` additionally exercises
the imported artwork. The synthetic workload diagnostics remain at
`/diagnostics.html`; `/health` reports server health and `/ws` serves the
versioned diagnostic protocol. See [documentation](docs/index.md) for ownership,
checks, and known limits.

Original code is MIT licensed. See [third-party notices](THIRD_PARTY.md) for
asset boundaries.
