# Shared dev server

`make compare` and `make showcase` used to start a private "before" (dev)
server for every run, and every gameplay take prepared its map from scratch (a
real take spent 169 s of 203 s in `start_skirmish`). `make dev-server` keeps one
server built from `origin/dev` instead, and the prepared maps with it.

```sh
make dev-server          # build if dev moved, start or keep current, print the status line
make dev-server-status   # revision, port, uptime, maps ready, footprint
make dev-server-stop     # stop every dev-server process (builds and maps stay)
```

```
dev-server: running revision 1af90901095f, port 8091, uptime 3m12s, maps ready 2/2, footprint 9100 MiB (trees 80, target 9000, maps 20)
```

`STALE: dev is at …` is appended when `origin/dev` has moved on; the next
`make dev-server` rebuilds and restarts.

## What it is

Everything lives in `<git common dir>/aoe-dev-server/`, shared by every
worktree and owned by none: `trees/<revision>/` (a `git archive` of the revision
with its built `aoe-server` and `aoe-map-worker`; never a user worktree),
`target/` (one cargo target for all revisions, `incremental/` trimmed after
each build), `presets/<fingerprint>/` (prepared maps), `logs/`, `state.json`
and `lock`. `make dev-server` fetches `origin/dev` (best effort), builds the tip
in the tool image when its tree is missing, and starts the *play* server on
`127.0.0.1:8091` (`AOE_DEV_SERVER_PORT`). An advisory lock serialises builds and
starts, so a second session waits and then finds the server current. The server
runs detached from the command that started it; `/health` reports the revision
(`AOE_BUILD_SHA`), which is what "healthy at the expected revision" means. The
asset pack and geodata cache come from the invoking checkout, else the main
checkout (`local-assets/packs`, `.cache/geodata`).

Settings (environment): `AOE_DEV_SERVER_REV` serves any commit-ish instead of
`origin/dev` (proofs, bisecting), `AOE_DEV_SERVER_PORT`, `AOE_DEV_SERVER_PRESETS`
(more `<preset>:<size>` maps such as `alps:tiny,paris:small`),
`AOE_DEV_SERVER_PACK`, `AOE_DEV_SERVER_FETCH=0` (no fetch).

## Preset maps

Starting a skirmish submits a map request the server prepares with its worker.
The play server is started with `AOE_PRESET_MAPS`, a JSON list of requests
(`crates/server`, `map_jobs/presets.rs`): it prepares them one after the
other at boot, and from then on a job whose request and preparation plan
match a stored package (the checks a worker result passes) completes at once
from it, so `start_skirmish` skips map preparation. Without the variable the
server behaves as before.

The list is Finnish lakeland at `tiny` (what the showcase step library films),
the maps named in `qa/bookmarks.json`, and `AOE_DEV_SERVER_PRESETS`. The
requests are what the setup form submits (default scale 30, seed 1); a test
holds the preset table to `web/start/views.js`. Packages are kept under
`presets/<fingerprint>/`, where the fingerprint hashes the Git object ids of
the map and geodata crates, `crates/core`, the worker and plan code of the
server, `Cargo.lock` and the toolchain. A dev change outside that code keeps
the maps (the server restarts, the maps are reused); a change inside it starts
an empty directory and removes the old one. Preparing needs the geodata cache
and takes minutes the first time; `maps ready n/m` counts completed presets.

## Who uses it

- **Showcase.** A storyboard with `"build": "before"` scenes and no
  `SHOWCASE_BEFORE_URL` films the play server, when it is healthy at the
  revision `origin/dev` is at. Otherwise the error names why (not running,
  process gone, serves another revision, no answer) and the explicit setting
  still works as before. Start the take's skirmish on it with the step
  library ([showcase](showcase.md)); the second take finds its map prepared.
- **Compare.** The `before` side uses the shared build when the merge base is
  `origin/dev` itself: it builds that revision once under the dev server
  (instead of per branch) and keeps one *view* server per distinct map,
  packages directory and asset pack, started on first use (ports 8092 up, at
  most three, the oldest stopped). Otherwise, or if that fails, it builds
  and starts its own as before and prints why. The `after` side is always the
  working tree. `report.json` records `dev-server` as the source.

## Limits

One play server holds one controller lease, as any server does: two filmings
at once need the explicit `SHOWCASE_BEFORE_URL` of another server. The revision
a branch compares against is its merge base, so a branch that is behind
`origin/dev` falls back to a private build. The footprint is the cargo target
(several GiB, kept for fast rebuilds), the exported trees of the revisions in
use, and the prepared maps; `make dev-server-stop` frees memory, deleting
`aoe-dev-server/` frees the disk. Reuse trusts the package directory: it never
holds packages of older map code because the directory is keyed by fingerprint.
