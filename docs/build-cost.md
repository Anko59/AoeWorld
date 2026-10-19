# Build cost per worktree — 2026-10-14

Measured source: `1a95acca8f0fdff0aaf8585ebb094dbf1cc367b6`, branch
`harness/compile-cache-measure`. Ryzen 5 4600H: 6 physical cores, 12 logical
CPUs, 14.48 GiB RAM, 4 GiB swap almost full. Other sessions remained active;
load varied substantially. These are observations on this host, not isolated
memory attribution, RTS performance, or dedicated-hardware qualification.

## Method

One Make target at a time, in detached scratch worktrees under the task
checkout; heavy work acquired the normal slot through `aoe-harness slot`.
Cold means empty compiler target with warm Cargo downloads. The baseline ran
real `make test-unit` (nextest plus doctests) and independent cold/warm
`make lint`. Load is the starting 1-minute load; available RAM is starting
MemAvailable. Drop is start minus the minimum MemAvailable, sampled every
2 seconds; size is allocated `du -sk` after completion. Wall time includes
Docker startup and slot waits. No jobs, tests, assertions, or deadlines changed.

The measured `DOCKER_RUN` set `CARGO_HOME=$ROOT/.cache/cargo`, bound the
checkout and external Git metadata, and defaulted to `$ROOT/target`.
`HARNESS_CARGO_CACHE` and `HARNESS_TARGET_CACHE` project external caches into
those paths. Today the Cargo cache defaults to `<git common dir>/aoe-cargo`,
used as `CARGO_HOME` and mounted read-write at its host path and the former
`$ROOT/.cache/cargo` location in Make containers. Registry archives,
sources, index and Git dependencies are shared across worktrees, gate snapshots,
red trees and harness builds. Concurrent Cargo processes safely share this home:
Cargo takes its own cache locks. Compiler targets remain per worktree.
`HARNESS_CARGO_CACHE` can explicitly select another cache.

All supervised Cargo captures (including WASM output retention, Nextest metadata
exit 102, and Docker builds for compare/dev-server) share one recovery path. An
offline missing dependency prints why, runs one online `cargo fetch --locked`,
then retries the original command once within its original deadline. The fetch
keeps the Cargo home, manifest, target, cwd and Docker image/mounts; only that
attempt overrides offline mode and a Docker `none` network. Fetch output stays
out of retained runner stdout and metadata JSON. Other failures do not retry. If the network is
unavailable, connect it and run `make cargo-fetch` in the failing checkout, then
rerun the gate. The restricted deployment worker retains its separate read-only
image cache and network isolation ([worker contract](restricted-local-worker.md));
prepare that image with its required dependencies.

The tools image and its Rust derivatives contain no sccache.
Measured image ID:
`sha256:b3bd432f92a243c403cd7e6558b97dd9f3eb74ffb5c9847fe441b3e3c54c8c04`.
Rust/Cargo 1.93.1, LLVM 21.1.8, GCC 12.2, nextest 0.9.144.

Cache experiments used disposable Make targets in that same Docker runner:
`cargo test --workspace --locked --no-run --timings`, canonical Cargo home
`/measure/cargo`, and a transparent linker wrapper recording `cc` elapsed
time. Stable Cargo HTML supplied unit timings; no nightly `-Z` flags were used.
Temporary fixture builds used a checkout-owned TMPDIR after the initial pilot.

## Results

All four baseline rows passed; times are seconds and memory/sizes are GiB.
The cold unit gate waited 412 seconds for its slot. Its native run passed
2,037 tests (2 skipped), and doctests passed; warm native tests also passed.
Warm tests still compile disposable canary programs.

| Target | Start (CEST) | Load | Available | Wall | Peak drop | Target size |
|---|---|---:|---:|---:|---:|---:|
| Cold test-unit | 14:27:45 | 20.59 | 6.58 | 1240.71 | 2.05 | 7.06 |
| Warm test-unit | 14:48:43 | 23.76 | 7.80 | 520.06 | 3.06 | 7.06 |
| Cold lint | 13:59:18 | 12.46 | 8.54 | 153.80 | 2.35 | 2.42 |
| Warm lint | 14:02:06 | 12.90 | 8.72 | 1.27 | 0.05 | 2.42 |

| Compile-only case | Start | Load | Available | Wall | Cargo | Slot wait | Drop | Size |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| Private cold target | 14:57:43 | 12.78 | 8.01 | 192.21 | 185.4 | 0 | 2.78 | 4.88 |
| sccache empty | 15:00:56 | 16.63 | 7.58 | 342.11 | 269.7 | 0 | 4.32 | 4.88 |
| sccache populated, fresh worktree | 15:06:39 | 21.71 | 8.62 | 711.01 | 205.7 | 503 | 3.41 | 4.88 |
| Whole shared target, seed | 15:18:33 | 26.60 | 7.34 | 221.37 | 208.3 | 0 | 4.06 | 4.88 |
| Whole shared target, second worktree | 15:22:15 | 32.81 | 6.92 | 42.62 | 1.38 | 38 | 0.55 | 4.88 |
| Shared-target lock pair | 15:22:57 | 33.75 | 6.95 | 30.18 | 17.72 pair | 0 | 1.59 | 5.29 |

The second shared-target row reused every unit, but its executable scope was
wrong: an integration binary launched from worktree 2 checked a deliberately
broken document present only in worktree 1. This speedup is **not a correct
worktree-2 gate**. Cargo's build lock also ends before test execution, leaving
shared executable paths exposed to another build.

The lock pair ran two Cargo clients inside one queued Make target. The second
reported `Blocking waiting for file lock on artifact directory` and waited
about 14.92 seconds while the first compiled a harmless scratch source edit.
Only one compilation was active; both exited 0.

Sccache 0.18.0 was a volume-mounted release binary, archive SHA-256
`45f1447fbe231e3037bde351ef70677dd212216c8d62ae7ca409fecc4d6acc89`.
It used a 10 GiB disk limit and `SCCACHE_BASEDIRS=$ROOT`; incremental compilation
remained enabled. The second run had 217 hits (212 Rust), 31 misses, and zero
cache errors; stored data grew from 175 to 211 MiB. Dependency unit time fell
from 615.2 to 209.5 seconds, but Cargo wall time increased 11%. No overall
speedup or memory reduction was demonstrated. The empty-cache invocation also
had about 63 seconds after Cargo completion for statistics/server shutdown
and container completion; wall and Cargo times remain separately reported.
[Supported Rust caching](https://github.com/mozilla/sccache/blob/main/docs/Rust.md)
excludes incremental compilations and final executable links.

## Where compilation goes

The cold native profile contained 343 dependency/build units and 48 workspace
units: summed elapsed unit times were 615.21 and 731.02 seconds respectively
(45.7% and 54.3%). These overlap and are not CPU seconds or additive wall time.
Dependency units finished by 61.79 seconds; the whole build took 185.4 seconds.

| Largest cold units | Seconds |
|---|---:|
| aoe-server library tests | 95.91 |
| aoe-map library tests | 81.10 |
| aoe-harness unit-test binary | 81.08 |
| aoe-harness regular binary | 68.33 |
| aoe-server library | 52.67 |
| Tokio 1.53.2 | 19.39 |
| regex-automata 0.4.18 | 15.81 |
| Syn 2.0.119 | 14.28 |
| Syn 3.0.6 | 13.72 |
| clap_builder 4.6.7 | 13.71 |

The single harness unit-test binary contains 1,147 registered tests.
Harness test compilation including linking took 81.08 seconds; the external
link call took 1.66 seconds, leaving about 79.42 seconds of compiler work.
The regular binary took 68.33 seconds, with a 1.35-second link call. Splitting
an evenly balanced four-way test crate could reduce a cold recompilation after
an edit in one area from 81 to roughly 22 seconds: an optimistic saving of
about 60 seconds. Common compilation overhead, integration binaries and warm
incremental edits limit that estimate. The lock probe's comment-only warm
invalidation rebuilt the harness test unit in 15.52 seconds and its regular
binary in 13.32 seconds, already much faster than the cold units. No semantic
area edit or split was implemented or benchmarked.

## Recommendation and limits

**One next step:** prototype a shared native target with worktree-specific
workspace artifact hashes (`RUSTC_WORKSPACE_WRAPPER`) and private captured
executables for gate execution. Aim to reuse unchanged dependencies while
compiling workspace crates for the actual worktree. The observed dependency
phase offers approximately 30–60 seconds of potential saving per fresh native
build; the 1.38-second whole-target result must not be used as its forecast.
Risks: Cargo lock contention, remaining workspace compilation memory, mutable
shared caches, embedded roots and unqualified final executable names. Validate
scope and concurrent artifact capture before changing default targets.

Stable Cargo 1.93.1 accepted `build.build-dir`: the small core probe placed
0.88 MiB of final output separately from 61.9 MiB of intermediates. It does
not select dependencies alone. Stable workspace-wrapper hashing is a possible
artifact-identity boundary, not a per-crate target-directory setting. A small
core probe using different wrapper paths produced distinct workspace artifact
hashes; the second worktree compiled only core in 1.49 seconds, reusing its
third-party dependencies. The full harness pipeline with this isolation was
not tested.
See [Cargo configuration](https://doc.rust-lang.org/cargo/reference/config.html#buildrustc-workspace-wrapper).

Initial pilots failed from Docker-created root-owned cache directories,
instrumentation environment-forwarding audits, or the unchanged 600-second
native deadline. They were retained separately and the clean baseline was
repeated successfully. No tool image, default target, test assertion or
production source was changed. Busy-host samples do not establish OOM safety;
release/WASM builds, coverage and preflight were not measured.
