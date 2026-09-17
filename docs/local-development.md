# Local development

The supported host is Linux x86-64 with Git, Make, Docker, and Docker Compose.
`make bootstrap` builds a pinned Rust image and installs Git hooks. Build caches
are ignored under `.cache/`; local trial assets belong under `local-assets/`.
`make help` lists implemented commands. The first bootstrap needs image and
crate downloads. Run `make dev` for the synthetic server and check `/health`.
