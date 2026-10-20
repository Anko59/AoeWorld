# Troubleshooting

If bootstrap fails, check `docker info`, available disk space, and access to
the pinned image and crates registry. If the server cannot bind, inspect the
host port 8080. An unsupported WebGPU adapter should be reported as a browser
capability failure with the browser version and platform.

If a browser reports visible sprites but the canvas is blank, run
`make test-e2e` and inspect its full-page screenshot. The pinned browser gate
starts Chromium under Xvfb with GPU compositing and Vulkan SwiftShader; adapter
creation or draw counters alone do not prove that the canvas was presented.
See the [documented Chromium compositor failure and working flag set](https://github.com/visgl/luma.gl/issues/2874).

## Harness disk use

Hook builds retain two judge revisions and clean their shared Cargo target
under `build.lock`: incremental output first, then `cargo clean` above 2 GiB.
After a ship pushes and arms
auto-merge into dev, it removes this worktree's `target/debug/incremental` (not the
snapshots, which a concurrent check may still use) and prints the file bytes freed. Failed ships retain
their builds; a later reship recompiles. A killed process or filesystem error
can leave output requiring manual cleanup after verifying no run uses it.

## The machine slept during a run

A host suspend stops every process, but deadlines count monotonic time, which
does not advance while the machine sleeps. The harness therefore compares
CLOCK_BOOTTIME with CLOCK_MONOTONIC: their difference (the boot-time gap) grows
by the length of each suspend. A supervised child whose gap moved by more than
60 s since it started is killed and reported as interrupted, for example
`make test-unit was interrupted by a host suspend at 2026-10-17 07:10:00Z`.
The child receives its start gap in `AOE_HARNESS_BOOT_GAP`.

- **Gates run by `aoe-harness gates`** retry once. Their harness containers are
  stopped first, because killing the `docker run` client does not stop the
  container. A second suspend is the gate's result, reported as a monitor error.
- **Ship gates and duel steps** do not retry. An interrupted one fails and its
  process group is killed; containers it started are labelled (below) and
  `make recover` can stop them.
- **Containers** started by the harness carry the label
  `aoe.harness-boot-gap=<seconds>`, the gap when they started. These are the e2e
  browser run, the wasm driver, and the `DOCKER_RUN` and `SLOT_RUN` recipes under
  a supervised `make`. `make recover` stops the labelled containers whose gap has
  moved, names each one, and still stops the others when one removal fails.
  Plain `docker run` recipes without the label (for example `GEODATA_RUN`,
  `BROWSER_RUN` and `SOURCE_QUAL_RUN`) are not found by `make recover`.

**Not yet automatic:** a ship lock held across a suspend is not taken over.
The lock is an OS file lock, so waiters keep waiting while the holder process
exists, even when it was frozen by the suspend. To recover, run `make ship-status`,
which names the holder, and kill that process. The waiters then take the lock.

Limits: detection needs Linux; elsewhere the clocks are not read and nothing is
judged interrupted. The tests inject clock readings and do not suspend a real
machine, so the timing of a real resume is not verified.
