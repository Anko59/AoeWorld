# 5x and 10x benches (owner data stays in `.cache/`)

Goal (owner): benches as close to the owner's Anatolia 4 game as possible (73,617 units, 99 AIs,
2,000-tile map), at 5x and 10x, with long games on large maps and many AIs. This page is the recipe;
results of a given run go in its report, not here. Saves, packages and recordings stay out of Git.

## Large map package (4,500 tiles, owner density)

The owner's density is 960 km at 240:1 compression for 2,000 tiles. 5x the area is 2,160 km:

```sh
cat > .cache/bench5x/request.json <<'J'
{"schema_version":1,"center_latitude_e7":390000000,"center_longitude_e7":340000000,
 "requested_side_meters":2160000,"compression":{"numerator":240,"denominator":1},
 "year_ce":600,"seed":1,"reconstruction_profile":"circa600_v1"}
J
AOE_MAP_REQUEST=.cache/bench5x/request.json AOE_MAP_PACKAGE=.cache/bench5x/packages make map-generate
AOE_MAP_PACKAGE=.cache/bench5x/packages/<hash>.json make map-verify
```

From the hard-linked `.cache/geodata` this took under 90 s (9 sources, no download) and made a 69 MB
package. Known limit (2026-11-25): a game on it stalls, see below.

## Seeding and running

`scale-match --late-game --scale S` seeds the owner's late game times S (S = 5.7 gives about 53k units on
any map; 28.5 about 265k). The AIs then grow it. Flags added for long runs: `--save-as NAME` (saves when the
run ends, and whenever `target/scale-match/save-now` appears; the file's first line names the save),
`--nice` (server at nice 19, bot containers at low CPU shares), `--no-march` (no scripted army march).

`make scale-match` runs the harness in a 4 GiB container, and the server lives inside it, so a game over
about 150k units is OOM-killed there. Run the harness on the host inside a long-job slot, with the
server already built (a no-op `cargo` first in PATH skips the rebuild):

```sh
mkdir -p .cache/bench5x/shim && printf '#!/bin/sh\nexit 0\n' > .cache/bench5x/shim/cargo && chmod +x .cache/bench5x/shim/cargo
make scale-match-smoke   # builds target/release/aoe-server and aoe-harness (any other build that leaves both works)
PATH=$PWD/.cache/bench5x/shim:$PATH .cache/resource-runner --long scale-match -- \
  target/release/aoe-harness scale-match --late-game --no-march --scale 28.5 --players 100 --humans 1 \
  --tiles 2000 --map package --map-hash <hash> --package-directory .cache/sr/packages --hosts 8 \
  --minutes 125 --port 9388 --nice --save-as bench5x --label bench5x --no-summary --out .cache/bench5x/growth.json
```

Retry on exit 75/76 (slots busy). Log every 10 minutes: `GET /match/stats` (last row: tick, units),
the server's `VmRSS`, `MemAvailable`. Stop before MemAvailable falls below 3 GB: write a save name into
`target/scale-match/save-now`, wait for `scale-match: saved ...`, then stop the run.

## After the save

Resume it with `--save <package-dir>/saves/<name>.save --record-commands <file> --record-ticks 750`, then
`make save-replay` with `--runs 2` on that save and stream; the two runs' hashes must match.
Hang diagnosis: run the server under `gdb -batch -ex run -ex 'thread apply all bt 16'` through a
`nice` shim and send SIGINT when `/match/stats` stops advancing (ptrace attach is blocked here).

## Known blockers found while building these benches

- Walking-field panic with offices past a guide's box: fixed on `fix/walking-field-bounds`.
- 4,500-tile maps: `Regions::connected` (unbounded search) inside a bot's formation order blocks the tick
  for minutes when the target is unreachable. Not fixed; the 4,500-tile game cannot run until it is.
- Restore: the first 5x save (227,676 units, 109,331 buildings) fails to resume with "0 units and 2
  buildings could not be placed"; restore is all-or-nothing. Fix in progress on `fix/restore-unplaced`.

## What the first 5x run measured (2026-11-25)

Anatolia 61633e71 (2,000 tiles), `--late-game --scale 28.5 --no-march`, 99 AIs, server at nice 19 under gdb on a
loaded machine (ticks/s not representative):

| minute | units | RSS | KB per unit |
| ---: | ---: | ---: | ---: |
| 0 | 264,601 | 1.79 GB | 6.8 |
| 10 | 243,489 | 1.98 GB | 8.1 |
| 34 | 228,808 | 1.78 GB | 7.8 |

The AIs lose units to combat faster than they build, so seeding at 5x does not grow towards 370k; a 5x state
needs AI economies that grow. At 7-8 KB per unit, 10x of the owner's game (about 740k units) needs 5.5-6 GB of
server RSS: it fits on the 14 GB machine, after the 4,500-tile stall, the restore failure and growing AI
economies are solved; `make scale-match` caps the harness container at 4 GiB, so 10x must run on the host.
