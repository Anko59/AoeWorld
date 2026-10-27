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

Local `make ship`, `make test-e2e`, `make test-wasm`, `make test-unit` and
`make preflight` refuse before taking a heavy slot when either the worktree's
target filesystem or the shared `<git common dir>/aoe-cargo` filesystem has
less than 15 GiB available. `AOE_DISK_FLOOR_GIB` sets a whole-GiB floor;
`0` disables it. CI (`CI` or `GITHUB_ACTIONS` nonempty) skips the check.
The refusal gives available bytes, the floor, and the five largest `target/`
directories from `git worktree list`, largest first, with file-byte sizes.
`make clean-stale` lists every worktree as MERGED (tip in `origin/dev`, or a
merged pull request with that head), IDLE (nothing modified for 3 days) or
ACTIVE, with `target/` size; `APPLY=1` removes merged worktrees and only the
`target/` of idle ones. It never touches the main checkout, the current
worktree, one with uncommitted changes, one a process has as its working
directory, or a merged one modified in the last hour. `origin/dev` is not
fetched. `make ship` names it when under 50 GiB is free.
`make disk-precheck` checks without starting a gate. Its bounded command
bootstrap still needs enough disk to build the checking binary on a cold start.
This is an admission check; other processes can consume disk afterward.

Showcase videos are required for player-visible changes. All of `browser/`
is test/QA infrastructure (specs, fixtures, reporters and Playwright config),
so changes there alone do not require one. Player-facing `web/` code still does.

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
- **Ship gates** (`make ship`) get one suspend retry per gate, including when the
  interrupted attempt followed an overloaded timeout. If stopping interrupted containers
  fails, the gate is unavailable, names the containers, and starts no retry. **Duel steps** do not retry: an
  interrupted one fails and its process group is killed; containers it started are
  labelled (below) and `make recover` can stop them.
- **Containers** started by the harness carry the label
  `aoe.harness-boot-gap=<seconds>`, the gap when they started. These are the e2e
  browser run, the wasm driver, and the `DOCKER_RUN` and `SLOT_RUN` recipes under
  a supervised `make`. `make recover` stops the labelled containers whose gap has
  moved, names each one, and still stops the others when one removal fails.
  Plain `docker run` recipes without the label (for example `GEODATA_RUN`,
  `BROWSER_RUN` and `SOURCE_QUAL_RUN`) are not found by `make recover`.

**The ship lock.** The lock is an OS file lock, so waiters keep waiting while the holder
process exists, even when a suspend froze it. The holder records the boot-time gap when it
takes the lock. While supervising a child, the holder compares the current gap with its last
lock record on every poll, and refreshes that record when the difference exceeds 60 s.
This includes several short sleeps across different children. A waiter ends a holder only when
the record's gap moved by more than 60 s since it was written and the same record still
reads that way at the next poll (one second later), by killing the recorded pid if its
start time matches. It prints `ship: taking over from the stale lock holder <branch> in
<worktree> (pid N): ...`. A holder that runs no supervised child across a suspend is ended.

Limits: detection needs Linux; elsewhere the clocks are not read and nothing is
judged interrupted. The tests inject clock readings, or write seconds into the file named by
`AOE_HARNESS_SIMULATED_GAP_FILE` (added to the boot clock only in test builds; production
ignores it); they do not suspend a real machine, so the timing of a real resume is not verified.
