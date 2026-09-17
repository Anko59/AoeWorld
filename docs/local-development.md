# Local development

The supported host is Linux x86-64 with Git, Make, Docker, and Docker Compose.
`make bootstrap` builds a pinned Rust image and installs Git hooks. Build caches
are ignored under `.cache/`; local trial assets belong under `local-assets/`.
`make help` lists implemented commands. The first bootstrap needs image and
crate downloads. `make dev` builds the Rust/WASM client and server, starts this
checkout's synthetic lab in a detached, read-only application container, and
waits for `http://127.0.0.1:8080/health`. Use `make status` and `make logs` to
inspect it, then `make down` to stop only this checkout's container. The health
endpoint and client diagnostics show the checkout commit; a modified tree adds
`-dirty` to that build identity.
