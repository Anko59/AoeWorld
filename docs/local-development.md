# Local development

The supported host is Linux x86-64 with Git, Make, Docker, and Docker Compose.
`make bootstrap` builds a pinned Rust image and installs Git hooks. Build caches
are ignored under `.cache/`; local trial assets belong under `local-assets/`.
`make help` lists implemented commands. The first bootstrap needs image and
crate downloads. `make dev` builds the Rust/WASM client and server, starts this
checkout's AoeWorld server in a detached, read-only application container, and
waits for `http://127.0.0.1:8080/health`. Use `make status` and `make logs` to
inspect it, then `make down` to stop only this checkout's container. The health
endpoint and client diagnostics show the checkout commit; a modified tree adds
`-dirty` to that build identity.
`DEV_PORT` (default 8080) chooses the loopback host port, as in
`DEV_PORT=8082 make dev`; the server still listens on 8080 inside its
container. Containers are named per checkout, so two worktrees can each run a
server on their own port, and `make down` stops only its own. Changing the
port of a running server needs `make down` first.
Set `AOE_ASSET_PACK=local-assets/packs/<pack-hash>` for `make dev` to enable the
game and the imported sprite viewer described in [assets](assets.md).
