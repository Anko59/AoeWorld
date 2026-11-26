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

With OrbStack instead of Colima, use a Linux machine named `aoeworld`
(`AOE_ORB_MACHINE` names another). It mounts `/Users` at the same path and
reaches OrbStack's Docker engine through the guest socket, so containers see
the checkout at its Mac path. `colima.sh` uses it when no Colima VM exists:

```sh
orb create ubuntu:noble aoeworld
orb -m aoeworld sh -c 'sudo apt-get update && sudo apt-get install -y make git ripgrep docker.io'
orb -m aoeworld sh -c 'sudo systemctl mask --now docker.socket docker.service'
orb -m aoeworld sh -c 'sudo groupadd -g "$(stat -c %g /opt/orbstack-guest/run/docker.sock)" orbdocker && sudo usermod -aG orbdocker "$USER"'
orb -m aoeworld sh -c 'echo "L+ /run/docker.sock - - - - /opt/orbstack-guest/run/docker.sock" | sudo tee /etc/tmpfiles.d/orbdocker.conf && sudo systemd-tmpfiles --create'
orb -m aoeworld sh -c 'sudo install -m 0755 .agents/hooks/orb-mac.sh /usr/local/bin/aoe-mac && for n in gh claude ssh aoe-mac-sign; do sudo ln -sf aoe-mac /usr/local/bin/$n; done'
orb -m aoeworld sh -c 'sudo git config --system gpg.format ssh && sudo git config --system gpg.ssh.program aoe-mac-sign && sudo git config --system user.signingkey mac && sudo git config --system commit.gpgsign true'
.agents/hooks/harness.sh build
```

The machine holds no login, token or key: `gh`, `claude`, `ssh` and commit
signing run the Mac's own programs through OrbStack's `mac` command
(`.agents/hooks/orb-mac.sh`), which keeps the directory, standard streams and
exit status and passes the harness's variables. Signing uses the key the Mac's
Git configuration names for the checkout. An `origin` cloned through an SSH
host alias (`git@github-perso:owner/name`) is resolved with `ssh -G`. `AOE_GH_ACCOUNT=<login>` on a `make`
command picks one of the Mac's `gh` accounts, for example the one with push
access when the active account has none. Only paths under `/Users` are visible
to containers, so leave `TMPDIR` unset in the machine.

After that, work on the Mac as on Linux. `GNUmakefile` sends every `make` goal,
with its command-line variables, to the VM; `.agents/hooks/harness.sh` sends
every agent hook there, so Claude Code and the other runtimes are judged on a
Mac exactly as on Linux. Both go through `.agents/hooks/colima.sh`, which runs
one command in the VM in the current directory and returns its output and exit
status. It forwards the harness's variables (`AOE_*`, `HARNESS_*`, `SHIP_*`,
`REVIEW_*`, `PERF_*`, `SHOWCASE_*`, `DEV_PORT` and the test-thread settings),
Git's hook context and your Git identity. `COLIMA_PROFILE` selects a VM other
than `default`.

### Shipping from the Mac itself

With Colima, `AOE_NATIVE_SHIP=1` (exported, or on the `make` command line) runs
`make ship`, `ship-status`, `describe-check`, `review`, `review-floor`,
`review-pr`, `issue`, `next`, `carry-review` and `update-prs` on the Mac, with its
own `gh`, `claude`, Git, SSH and commit signing, so the VM needs no login. A
`make` command that also names another goal runs in the VM. It is opt-in: a person
may be logged in inside the VM with another account. OrbStack ignores it, since
its machine already uses the Mac's programs.

The Mac then needs `gh`, the `docker` client (ship asks `docker info` before a
gate) and [rustup](https://rustup.rs) with the toolchain of `rust-toolchain.toml`
(`rustup toolchain install <channel> --profile minimal`; nothing is installed for
you). `.agents/hooks/mac-judge.sh` compiles the judge from `origin/dev`, never from
the checkout, and caches it under the same key as the Linux one, apart from it, in
`aoe-agent-hook/darwin-<arch>/` of the Git common directory. Every gate still goes
through `make` into the VM, and agent hooks, the reviewers' included, are still
judged there.

`AOE_GH_ACCOUNT=<login>` picks the account for `gh` and for Git over https. Its
token comes from `gh auth token` at each call, and reaches `gh` in that process's
environment and Git through a credential helper: never a file, a command line, Git
configuration, the reviewers or the gates.

### Limits

- The checkout must be under `$HOME`; nothing else is mounted in the VM.
- Tokens are not forwarded, because a command line is visible to `ps`. For
  `make ship` and `make issue`, set `AOE_NATIVE_SHIP=1`, or run `gh auth login`
  once inside `colima ssh` (an OrbStack machine uses the Mac's `gh` instead).
  The Mac has no `secret-tool` keyring; see [credentials](credentials.md).
- `make duel`, and `make ship` without `AOE_NATIVE_SHIP=1`, start agent
  command-line tools inside the VM, so install the ones you use there too.
- The judge of `AOE_NATIVE_SHIP=1` is compiled by the Mac's own toolchain and
  linker, not in the pinned image, and its dependencies' build scripts run on the
  Mac. CI does not build macOS: a change on `dev` can break that build.
- Each system has its own kernel locks: do not run a ship on the Mac and a ship or
  duel in the VM at once. `make load-status` shows the VM's slots; `ship-status` on
  the Mac does not. Work a killed gate started in the VM may outlive it.
- `SHIP_VIDEO` must be under `$HOME` for the VM to read it.
- `make dev` listens in the VM; Colima forwards its loopback port to the Mac, so
  `http://127.0.0.1:8080` works from the Mac's browser.
- Build output lives on the shared mount, which is slower than a Linux disk.
- Run `make test-e2e` on its own. The browser container uses host networking,
  and Chromium aborts page loads with `net::ERR_NETWORK_CHANGED` whenever
  another container starts or stops in the VM, so a spec then waits on
  "Loading…" until it times out.
- Keep 15 GiB free on the VM's Docker disk (`colima ssh -- df -h
  /var/lib/docker`): below that floor the disk pre-check refuses to start jobs
  and four harness tests that run the real ship command fail.
- On Apple Silicon the VM and the tool images are arm64. The Rust tools image
  pins arm64 builds of `cargo-nextest` and `sccache`; Chrome for Testing
  publishes no Linux arm64 `chromedriver`, so `make test-wasm` runs in CI only.
- A Git hooks path set in your global configuration (`core.hooksPath`) takes
  precedence over the repository's hooks on the Mac; run `make pre-commit`
  yourself before committing, or commit with
  `.agents/hooks/colima.sh git commit`.
