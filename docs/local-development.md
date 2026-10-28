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
The dev container has a hard 5 GiB memory limit, including its AI hosts and
map-worker children, with no additional swap allowance. An oversized match
is stopped by Docker rather than exhausting the host's RAM and swap.
After `make dev` has built the checkout, `make dev-start` starts those existing
artifacts in the same checkout-scoped container. Use it after `make down` to
change scenario settings without rebuilding; source changes still need `make dev`.
Set `AOE_ASSET_PACK=local-assets/packs/<pack-hash>` for `make dev` to enable the
game and the imported sprite viewer described in [assets](assets.md). The pack is
verified first; one converted by an older importer is refused with the
`make assets-import` step instead of leaving the page on "Unavailable". A pack
with building recipes but no attached Definitive Edition overlay (its
`buildings` link) is refused with the `make assets-import-de` step, because it
would draw squares for bridges, galleys and kings.
