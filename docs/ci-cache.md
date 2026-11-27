# CI caches and their budget

The repository's Actions cache holds 10 GB; GitHub evicts the least recently used entry when it is
full. On 2026-11-26 it held 10.13 GB in 4,864 entries: Rust caches 7.64 GB (6 entries, three of them
older generations nobody read), Docker layers 2.41 GB, and 4,002 entries (87 MB) written by pull
request runs. The Actions cache is also where the owner's game map package lives
([`game-map.yml`](../.github/workflows/game-map.yml), 66 MiB, read twice a week): an entry that is
not read for days is the first to go when the cache is full, so the Rust caches must stay far
below the limit.

## What is cached

| cache | key | written by | read by |
|---|---|---|---|
| Rust build, one per leg: `checks`, `unit` (the two `static` legs), `browser`, `e2e`, `perf` | `rust-shared-<leg>-<Cargo.toml>-<Cargo.lock, toolchain, tools Dockerfile>` | the job of the leg on a push to dev, when its key is new (`e2e`: shard 1 only) | the same job on every run; a new lock file falls back to the newest entry of the leg |
| Docker layers, one scope per Dockerfile | BuildKit `buildkit-blob-…`, `index-<scope>-…` | every job on a push to dev (`DOCKER_BUILD_CACHE_SAVE`) | every job |
| game map package | `game-map-<request>-<generator>` | `game-map.yml` | the game map actions |

## The rules that keep it small

- Only a push to dev writes a cache. A pull request restores and never saves (Rust and Docker).
- A Rust cache holds the dependencies and not the workspace crates: a checkout gives every source
  file a new modification time, so cargo rebuilds `aoe-*` whatever the cache holds
  (`rust-cache-save` deletes those artifacts and the test executables before saving).
- Legs that build the same thing share one cache: the four `e2e` shards restore `e2e`.
- After each validation run on dev, `cache-trim.yml` keeps the newest entry of each leg and deletes
  the older generations (`.github/actions/cache-trim`). It runs no pull request code and is the only
  workflow with `actions: write`. It never lists `game-map-…` or the Docker layers.
- A new leg needs a line in the table above and a measured size.
