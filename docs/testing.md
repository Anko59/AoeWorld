# Testing

Use `make fmt-check`, `make structure-check`, `make lint`, and `make test-unit`
(`make lint` also compiles the test targets of the wasm32 packages `make test-wasm` runs, so wasm-only test code cannot break unnoticed; clippy is not denied for wasm32 yet)
for focused checks. `make pre-commit` runs static gates; `make preflight` adds
native tests through pinned cargo-nextest, followed by separate doctests and a
WASM build. The test profile optimizes map, simulation, server, and procedural
hashing (BLAKE3) code for the
full-distance offline movement/replay regressions; debug assertions and integer
overflow checks remain enabled. Distances, tick limits, and gate floors are
unchanged. Local `make preflight` and the preflight gates of `make ship` run `test-unit`
only for the affected crates: the crates owning the files changed since the merge
base with `origin/dev`, plus every workspace crate that depends on them,
transitively (`cargo metadata`). A file outside every crate (root `Cargo.toml`,
`Cargo.lock`, toolchain, `.cargo/`, `gates/`, `Makefile`) selects the whole
workspace; a change under `web/`, `browser/` and `docs/` only runs no Rust unit
tests and says so. The first and last lines of the run, and so the gate
evidence, show `test-unit: N of M crates (changed: a, b; dependents: c, d)`.
CI (`GITHUB_ACTIONS=true`), nightly, a plain `make test-unit` and `AOE_TEST_ALL=1`
always run the full suite.

CI runs `make preflight` as two parallel legs of its `static` job (#388):
`make preflight-checks` (`deny`, `build-wasm`, the static checks and clippy) and
`make preflight-unit` (`test-unit`, then the smoke workload), the harness's
`preflight --part checks|unit`; together they run every gate `make preflight`
runs, and the job is green only when both legs are. Each leg restores the cargo
registry and `target/` cache that the last push to `dev` saved (key: leg,
`Cargo.lock`, toolchain pin and tools Dockerfile; restore-key: the leg alone);
pull requests only read it, so they cannot evict it from the repository's 10 GB.

**Source-qualification tier (#388).** `make test-unit` excludes, and
`make test-source-qualification` runs exactly, the slow offline source
qualification tests listed in `SOURCE_QUALIFICATION` in
`crates/harness/src/native/tiers.rs` (one list builds both nextest filtersets, so
a test is in exactly one run; the new target fails when no test matches, and a
harness test checks every listed name is a real test function):

| Test (aoe-server) | Quiet-machine time |
| --- | --- |
| `source_qualification::tests::orchestration::synthetic_flat_sahara_geographic_movement_exercises_full_routes_and_replay` | 74-90 s |
| `source_qualification::tests::orchestration::synthetic_flat_sahara_package_exercises_full_offline_qualification_contract` | 24-72 s |
| `source_qualification::movement::tests::continuous_source_free_run_completes_all_repeated_waypoints` | about 51 s |

The tier is a `qualification` gate (`gates/registry.json`), so it must run
post-merge or nightly; a PR that changes walkability, pathing or source
qualification should also run it locally. The tests stay byte-identical.

**Exhaustive tier (#388).** `make test-unit` also excludes, and
`make test-exhaustive` runs exactly (`--no-tests fail`), the tests listed in
`EXHAUSTIVE` in the same `tiers.rs`: the 16-seed landscape sweeps and the
full 1024-axis geodata preparations and round-trips. The `exhaustive` gate has
the `ci` and `nightly` cadences (it blocks both, like `coverage`; it is not
source qualification, so it has no `qualification` cadence) and a CI job like
`qualification` (pushes to `dev` and `main`, nightly, never pull requests); a
harness test checks every listed name is a real `#[test]` function. Where a
test sweeps seeds or axes, a fast variant of the same assertion stays in
`make test-unit`, so a gate change or a regression visible on two seeds is
still caught per PR. Measured under load (12 cores, load 11-16):

| Moved to the tier | Time | Per-PR counterpart in `test-unit` |
| --- | --- | --- |
| `landscape_patches::tests::wilder_regions_..._on_all16_seeds` | 62 s | `..._on_two_seeds` (seeds 0 and 255, same bounds) |
| `landscape_patches::tests::stand_edges_do_not_align_to_chunk_cell_or_lattice_grids` | 61 s | `stand_edges_do_not_align_to_grids_on_two_seeds` |
| `landscape_patches::tests::openings::most_forested_country_..._on_all16_seeds` | 52 s | `..._on_two_seeds` |
| `landscape_patches::tests::openings::groves_and_forests_below_the_large_radius_are_solid` | 47 s | `..._are_solid_on_two_seeds` |
| `landscape_patches::tests::cover_follows_a_wildness_gradient_without_a_step_at_cell_borders` | 39 s | `cover_follows_a_wildness_gradient_on_two_seeds` |
| `overview::tests::vector_compose_tests::offline_gdal_overview_composes_typed_vectors_and_roundtrips_all_roots` | 165 s | none: one 1024-axis fixture; other `vector_compose` and `field_axes` tests cover worker options and coupled roots |
| `overview::tests::field_axes_tests::offline_field_axes_prepare_independent_canonical_pyramids` | 95 s | `..._at_reduced_axes` (its 128/32/64/16 iteration) |
| `overview::tests::field_axes_tests::landscape_water_and_vegetation_are_accepted_and_published_at_the_elevation_axis` | 95 s | `landscape_water_and_vegetation_axes_are_accepted_at_the_elevation_axis` (the axis acceptance asserts) |
| `hyde::hyde_tests::production_archive_stream_prepares_the_independent_1024_axis` | 75 s | none: the 2 and 128 axis HYDE tests run the same path; only the 1024 streaming windows are tier-only |
| `terrain::tests::rivers::source_river_water_never_invents_invisible_crossings` | 20 s | none: `subcell_meandering_river_blocks_water...` asserts impassable, non-ford river water per PR; the tier adds the exhaustive unreachable route search |

Tests above 20 s in the aoe-harness crate (review calibration, QA source and
gate review) stay in `test-unit`: they are the only guard of the gate machinery.
`make test-wasm` runs the client browser integration tests and the
client and rendering library `wasm-bindgen-test` cases in pinned headless
Chromium through a disposable ChromeDriver container. On failure it retries the
browser suites exactly once when at least one named failed test reports a runner
WebGL context/graphics error (#391), even alongside value assertions or other test
failures. The log names the tests with context errors; the second full run decides
the gate. Only the existing narrow WebGL2 creation, lost-context, graphics,
shader, and link error messages qualify. Caught Rust panics include the runner's
`JS exception that was thrown:` / `RuntimeError: unreachable` section; that is
normal test failure detail. A pixel-only failure does not retry. Incomplete or
unrecognized diagnostics, compile errors, non-1 exits, cancellation, and timeouts
fail immediately. Complete failed-run stdout is kept
under `reports/process/` for classification alongside the usual bounded excerpt;
classification is limited to 8 MiB and requires a complete stderr tail. Native
suites and the duel's focused checks do not retry. Browser-only regressions
must use `#[wasm_bindgen_test]`; native `#[test]` cases in WASM-only modules do
not run in this gate. It writes
`reports/wasm/browser.json`; a successful WASM compilation alone does not pass
this gate. `make test-e2e` runs both an explicit WebGPU project under Xvfb with Vulkan
SwiftShader and a game-only project with default browser launch settings. It and
`make duel-check` first validate the stored packages in `local-assets/map-packages`;
packages from before a format change fail with an "invalid stored map package"
error, which names the files and says to regenerate them or move the directory aside.
In `make duel`, a wasm run that panics (a shared helper's assertion prints no test
name) or aborts after printing tests is a failed run, saved in `check.json` with the
panic's location and assertion. At green the implementer keeps its attempts and sees
them. At red a nameless panic counts only inside a file the tester changed; a panic that
follows a named, pre-existing failing test belongs to that test and never counts. Otherwise
round 1 refuses the check and keeps the tester's work, and an attack round drops the attack.
Game checks cover rendered pixels, movement, resize, and compatibility startup
after a missing WebGPU API, null WebGPU context, or unavailable adapter. WebGL2
checks require real shader pixels, depth/material parity, moving units, bound-
context startup failure, and idle/interactive context restoration. Explicit
software compatibility checks deny both WebGPU and WebGL2, keeping Canvas 2D
coverage rather than silently testing the accelerated tier.
The WebGPU project also checks actual canvas background and four sprite colors in a full-page
screenshot, along with reconnect and independent subscriptions.
`make test-memory-source` requires an already activated real-pack source map,
`AOE_POLISH_SOURCE_URL`, and `AOE_POLISH_SOURCE_HASH`. It checks a bounded camera
route on WebGPU, forced Canvas and default Chromium; missing inputs fail this
focused target. Ordinary E2E skips that external qualification when inputs are
absent. See [render polish](geodata/render-polish.md) for RSS bounds and limits.
`make coverage` records LCOV and JSON, excludes inline test modules from the
production-line denominator, and rejects less than 85% overall or 90% in
protocol, asset parsers, and policy/report logic. A missing group fails.
The coverage run instruments the native Playwright harness and server, and the
native browser WASM runner. It selects the server built by cargo-llvm-cov and
requires graceful shutdown so its execution counters are written. These are
native coverage records, not instrumentation of JavaScript or browser WASM.
Browser, WASM, asset, performance, QA, and artifact gates need
separate evidence.
The offline geographic regression runs all five 20 km orders and replay through
the same movement executor as source qualification, using persisted synthetic
pages. Source-specific northbound and connectivity diagnostics remain in
`make map-source-qualify` and focused diagnostic tests; the synthetic movement
fixture does not claim those real-source results.
`make fuzz-smoke` runs 512 libFuzzer cases against each DRS, SLP, palette,
pack-manifest, map-package/request, environmental-page, and compact-chunk parser.
The map campaigns begin with generated valid seeds; accepted package/page/chunk
values must retain their meaning and identity through serialization roundtrips. It uses a separately pinned nightly toolchain and keeps
new corpus inputs and crash artifacts outside Git. `make fuzz-nightly` gives
each target a 300-second campaign in five completed 60-second segments. Both write versioned reports under
`reports/fuzz/`; a discovered crash fails the gate.
When active corpus pressure warrants maintenance, the harness copies all active
inputs into ignored, content-addressed `fuzz/archive/` storage before running
coverage-guided `cargo fuzz cmin`. It restores canonical seeds afterward and
keeps crash artifacts untouched. Between nightly segments and after a completed
target, material corpus growth triggers archive and minimization before fuzzing
continues. If storage is still above 85% it tries other large corpora
and fails if headroom cannot be restored. During each target it watches the fixed active
corpus and artifact quotas and cancels the target at a 95% guard; cancellation
or fewer than five completed nightly segments is a failure. Reports record
segment counts, completed fuzz seconds, maintenance, each target's elapsed time
and outcome, and the storage snapshots. Archived inputs
are outside the active quota and must be retained separately.
`make mutation-nightly` runs pinned cargo-mutants against the impact
classifier, CI selection check, and 5% performance comparator. It requires a
completed, nonempty campaign with no missed or timed-out mutations and writes
`reports/mutation/nightly.json`. Mutations that cannot compile are reported
as unviable, separately from caught mutations.
Pull-request CI runs `make ci-select` against the base commit and records the
selected jobs in `reports/gates/selection.json`. Unknown paths and missing
base information select every job. The aggregate `required` check recomputes
the selection at the checked-out revision and requires success for each selected
job; only unselected jobs may be skipped. `make ci-check` validates that
contract locally when supplied the manifest and job results.

**Sharded browser suite (#388).** The Playwright suite ran on one serial worker
and took 19.5 of 20 minutes on the 4-core runner, behind `browser-check` and
`test-wasm` in the same job. CI now runs it as the `e2e` job, a matrix of three
parallel jobs, `AOE_E2E_SHARD=1/3`, `2/3` and `3/3`, beside the `browser` job
(`browser-check`, `test-wasm`). The registry's `e2e` job holds `test-e2e`, so it
is selected exactly when that gate is, and `required` takes the matrix result,
which is success only when every shard passed. Each shard builds its own server
and WASM bundle, then runs `make test-e2e`, which hands `--shard=i/n` to
Playwright; its container is named `aoeworld-e2e-shard-i-of-n-…`, it writes
`reports/e2e/shard-i-of-n.json` instead of the release evidence
`reports/e2e/pass.json`, and CI uploads `e2e-i-diagnostics`. Playwright splits
the tests, not the projects, so the shards together run every test once.
`make test-e2e` without `AOE_E2E_SHARD` runs the whole suite and writes
`pass.json` (the dev release job and `make coverage` use it); a shard together
with `AOE_E2E_COVERAGE` is refused. `AOE_E2E_SHARD` must be `i/n` with
`1 <= i <= n <= 16`; it does not apply to the duel's focused specs.

**Focused runs (`AOE_E2E_SPECS`).** To reproduce one spec through the
slot-managed command, name it: `make test-e2e AOE_E2E_SPECS=tests/village/selection-density.play.spec.ts`.
The value is a space- or comma-separated list of paths or globs under
`browser/tests` (`*` stays in one directory, `**` crosses directories); each
must select at least one `*.spec.ts`, or the run fails at once and names the
pattern. The harness passes the resolved specs to Playwright on one worker and
writes its JSON report to `reports/e2e/filtered.json` (recreated each run). A filtered run exits with Playwright's status, so a failing
selected test fails it; it expects no release evidence (`pass.json`) and no
renderer evidence, which only the whole suite writes. The harness creates
`reports/e2e/` itself. `AOE_E2E_SPECS` is refused when `GITHUB_ACTIONS` or `CI`
is set, so a filtered run can never count as the CI e2e, and it cannot be
combined with `AOE_E2E_SHARD`.

**One server per worker (#388).** `make test-e2e` starts `AOE_E2E_WORKERS`
disposable game servers (`1..=8`, default 3), each on its own free port and
with its own copy of the map store, and gives the browser container their URLs
in `AOE_BASE_URLS`. Playwright runs that many workers; the fixture in
`browser/server.ts` points a test at the server of its `parallelIndex`, so no
two workers share a controller lease or server-wide state. Every spec imports
`test` from that file, not from `@playwright/test`. `--shard` and workers
combine: each shard runs its tests on all workers. The coverage run keeps one
instrumented server and one worker (`AOE_E2E_WORKERS` above 1 is refused with
it), and the duel's focused specs run on one. Each worker also runs a software
WebGPU browser, so a loaded host can push a heavy test past its 30 s timeout;
lower `AOE_E2E_WORKERS` there. CI sets it to 1: on GitHub's 4-core runner three
servers per shard timed out heavy rendering and memory specs (#457's first run), so
the parallel workers serve local machines with more cores.
Do not claim a complete gate until they exist and pass at the exact revision.

## Failure output

When a supervised command fails, times out or is cancelled, the harness keeps
its full output in `reports/process/<program>-….log` and prints a failure
excerpt before the `exited with …; log: …` line: failing test lines, `panicked
at` with its assertion lines, `test result: FAILED`, and compiler `error`
lines with their location, de-duplicated and capped at about 40 lines and 4 KiB
(or the last 15 non-empty lines when nothing matches). Read the excerpt before
the log path: the last line of console output is often not the cause.

## Rust test placement

Tests live in `tests.rs`, `tests/`, or `*_tests.rs` (the agent role's test-file
conventions). `make test-placement` rejects added lines inside inline test
modules and `#[test]`-style functions in other Rust files under `crates/`,
including the harness. Use `#[cfg(test)] mod tests;` to load a separate file.
The comparison is against the merge base with `origin/dev`: existing inline
tests are grandfathered, and removing or moving them to test files passes.
Unparsable changed Rust files and unavailable comparison history fail clearly;
CI must fetch full history. Working checks include non-ignored untracked files;
index/commit checks use the same exported snapshots as other static checks.
The CLI supports `test-placement --index` and `--revision FULL_OID`.
The gate runs with the same cadences and blocking policy as `structure-check`.
Syntax analysis covers written Rust items, not items generated by macros.

## CI image builds

Every validation job first runs the `.github/actions/docker-setup` composite
action. It points the runner's Docker daemon at `mirror.gcr.io` for Docker Hub
pulls (base images stay digest-pinned; the daemon falls back to Docker Hub when
the mirror misses), switches to the containerd image store, and sets
`DOCKER_BUILD_CACHE=type=gha,version=2`. The Make image targets then pass
`--load --cache-from/--cache-to` with one GitHub Actions cache scope per
Dockerfile, so unchanged tool images are restored from layers rather than
rebuilt; tags, Dockerfiles, and every job and gate are unchanged. Cache export
failures are ignored and a missing cache only means a full build. Release jobs
(`dev-artifacts`, `release-candidate`, `main-promotion`) use only the mirror and
keep the daemon's default image store and uncached builds. Locally
`DOCKER_BUILD_CACHE` is empty and `docker build` arguments are unchanged.

## Registry and plans

[Registry v2](../gates/registry.json) owns path globs, suite implications,
prerequisites, cadence metadata, and the validation job map. Use `make gate-plan`
for a conservative PR plan and `make gates-docs` to regenerate the registry table;
`make test-harness` exercises the Rust harness and CLI regressions in Docker.
For red/green work, `make test-harness-focused HARNESS_TEST_FILTER=<substring>`
selects matching test names; it does not replace the full gate.
Unknown/protected paths and unavailable CI comparison bases select every job.
The manifest retains the requested and resolved base, canonical registry hash,
and dependency-first gate list; aggregate validation recomputes them all.
Every CI gate must directly identify an executing job, even when Make also runs
it as a prerequisite. Code changes now select parser fuzzing as additional work.

Plans do not imply automatic edit/stop interception. The opt-in local `gate-run`
executor is available; protected judging and CI migration remain subsequent work.
Existing Make/CI dispatch remains active; the static CI job and every handoff
still require the full `make preflight`, including for documentation changes.
Source/artwork/hardware checks are explicit qualification plans, never silently
substituted by synthetic tests. Release publication is not a generic cadence.
Pull requests into any branch receive the same validation, so a stacked pull
request is tested before its parent merges; protected branch publication is unchanged.

## Exact static inputs

`make pre-commit` now checks an exported index tree, not unstaged working bytes.
It preserves the existing formatting (including fuzz), structure, test placement, architecture,
docs, and strict workspace/all-target Clippy checks. Git's effective hook index
is forwarded into Docker: `commit -a` and pathspec commits use temporary indexes,
not necessarily the ordinary index. No source `write-tree` or index refresh runs.
Private Git metadata disables checkout conversions; raw file bytes and modes
are checked against that exact tree before/after each static check.

`make scope-check` reports index identity and affected paths without running gates.
A full resolved commit ID may be passed to the Dockerized CLI's `scope-check
--revision` option; symbolic refs are refused. Working preflight still checks
working source and runs the same native tests/smoke. Exported index/commit inputs
exclude ignored reports, original assets, untracked files, and copied caches; Cargo outputs stay
outside exported inputs. Linked worktrees and split indexes are covered.

These are consistency checks, not hostile same-user isolation or a trusted
judge. Reverted edits between probes, mutable caches, arbitrary candidate build
scripts, and source bootstrap compilation remain limits. Bare/unborn roots,
non-UTF-8 paths, conflicts, intent-to-add, symlink/gitlink input paths, and indexes
outside this checkout's Git metadata fail closed. Shared cadence execution and
protected-base authority remain subsequent stack work; no provider hook is
claimed installed.

## Local shared execution and evidence

`make gate-run HARNESS_CADENCE=edit HARNESS_SCOPE=working` runs the selected
named checks in deterministic prerequisite order. Before first use, build
`make tools orchestrator-tools` and supply an existing user-owned evidence
directory outside the repository and all caches via `HARNESS_EVIDENCE_DIR`.
The default `/tmp/aoeworld-harness-evidence` must also be prepared/owned by the
caller; Docker auto-created root-owned directories are not a writable substitute.
The evidence mount is added only to this bootstrap coordinator, not normal gate
containers. Direct Dockerized `gate-run --job JOB --output DIRECTORY` runs all
declared validation-job members and prerequisites, including static CI's full
preflight; `--scope commit --revision FULL_OID` selects a committed input.

Every selected gate gets PASS, FAIL, UNAVAILABLE, or SKIPPED. Missing retained
success output, missing capabilities, blocked dependencies, and budget exhaustion
are not PASS; changed input identity makes the entire ledger INVALID. Logs retain
bounded stdout and stderr even on success, with digests and truncation flags.
Ledger/log writes are atomic, mode 0600, fsynced, and outside declared writable
input/cache mounts. The versioned semantic BLAKE3 registry hash is shared with CI
selection; set-like arrays are sorted and all scheduling policy is bound.

Linux SIGINT/SIGTERM cancel the CLI's owned command group and retain non-PASS
results. Pipe reads cannot wait indefinitely on escaped descendants: nonblocking
per-poll quotas and a 200 ms drain grace bound captured output; escaped sessions,
Docker-daemon work, and uninterruptible kernel waits are separate limits. The
CLI includes scope preparation/image observations/final verification in elapsed
time and subtracts preparation from execution budget, but Git endpoint probes
are not themselves wall-supervised. External Cargo/target caches are projected
into exported containers rather than copied, and remain mutable inputs.

This ledger always says `authoritative: false`, even when every named command
passes. Candidate bootstrap/Make and same-user filesystem access can forge
local evidence. Post-run immutable Docker ID observations do not prove the
images a hostile Make used. Source-assets, real-geodata, and hardware capabilities
remain unavailable without qualified inputs; no environment flag manufactures
qualification. Existing CI and preflight gates remain unchanged.
