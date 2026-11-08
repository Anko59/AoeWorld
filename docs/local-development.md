# Local development

The supported host is Linux x86-64 with Git, Make, Docker, and Docker Compose.
A Mac works through [Colima](#macos-with-colima).
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

## macOS with Colima

The harness, its hook judge and its resource runner are Linux programs, so on a
Mac they run inside the Colima VM rather than on the host. Colima mounts the home
directory in the VM at the same path, so a checkout under `$HOME` is one tree
seen from both sides. Set it up once:

```sh
colima start --cpu 8 --memory 16 --disk 100
colima ssh -- sudo apt-get update
colima ssh -- sudo apt-get install -y make git ripgrep gh
.agents/hooks/harness.sh build
```

After that, work on the Mac as on Linux. `GNUmakefile` sends every `make` goal,
with its command-line variables, to the VM; `.agents/hooks/harness.sh` sends
every agent hook there, so Claude Code and the other runtimes are judged on a
Mac exactly as on Linux. Both go through `.agents/hooks/colima.sh`, which runs
one command in the VM in the current directory and returns its output and exit
status. It forwards the harness's variables (`AOE_*`, `HARNESS_*`, `SHIP_*`,
`REVIEW_*`, `PERF_*`, `SHOWCASE_*`, `DEV_PORT` and the test-thread settings),
Git's hook context and your Git identity. `COLIMA_PROFILE` selects a VM other
than `default`.

Limits:

- The checkout must be under `$HOME`; nothing else is mounted in the VM.
- Tokens are not forwarded, because a command line is visible to `ps`. For
  `make ship` and `make issue`, run `gh auth login` once inside `colima ssh`.
  The Mac has no `secret-tool` keyring; see [credentials](credentials.md).
- `make ship` and `make duel` start agent command-line tools inside the VM, so
  install the ones you use there too.
- `make dev` listens in the VM; Colima forwards its loopback port to the Mac, so
  `http://127.0.0.1:8080` works from the Mac's browser.
- Build output lives on the shared mount, which is slower than a Linux disk.
- On Apple Silicon the VM and the tool images are arm64. The Rust tools image
  pins arm64 builds of `cargo-nextest` and `sccache`; Chrome for Testing
  publishes no Linux arm64 `chromedriver`, so `make test-wasm` runs in CI only.
- A Git hooks path set in your global configuration (`core.hooksPath`) takes
  precedence over the repository's hooks on the Mac; run `make pre-commit`
  yourself before committing, or commit with
  `.agents/hooks/colima.sh git commit`.
