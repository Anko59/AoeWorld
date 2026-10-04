# Testing

Use `make fmt-check`, `make structure-check`, `make lint`, and `make test-unit`
for focused checks. `make pre-commit` runs static gates; `make preflight` adds
native tests through pinned cargo-nextest, followed by separate doctests and a
WASM build. The test profile optimizes map, simulation, server, and procedural
hashing (BLAKE3) code for the
full-distance offline movement/replay regressions; debug assertions and integer
overflow checks remain enabled. Distances, tick limits, and gate floors are
unchanged. `make test-wasm` runs the client browser integration tests and the
client and rendering library `wasm-bindgen-test` cases in pinned headless
Chromium through a disposable ChromeDriver container. Browser-only regressions
must use `#[wasm_bindgen_test]`; native `#[test]` cases in WASM-only modules do
not run in this gate. It writes
`reports/wasm/browser.json`; a successful WASM compilation alone does not pass
this gate. `make test-e2e` runs both an explicit WebGPU project under Xvfb with Vulkan
SwiftShader and a game-only project with default browser launch settings.
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
Do not claim a complete gate until they exist and pass at the exact revision.

## Registry and plans

[Registry v2](../gates/registry.json) owns path globs, suite implications,
prerequisites, cadence metadata, and the validation job map. Use `make gate-plan`
for a conservative PR plan and `make gates-docs` to regenerate the registry table;
`make test-harness` exercises the Rust harness and CLI regressions in Docker.
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
Pull requests targeting `harness/**` also receive validation while the upgrade
is reviewed as a dependent stack; protected branch publication is unchanged.

## Exact static inputs

`make pre-commit` now checks an exported index tree, not unstaged working bytes.
It preserves the existing formatting (including fuzz), structure, architecture,
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

## Protected-policy preparation (not judging)

The `policy-prepare` CLI and Make facade resolve the policy from an external
launcher anchor, never candidate `origin` or local `dev`. See
[judge preparation](judge-preparation.md) for source verification, setup and limits.
Preparation executes no gates, never replaces required CI, and always reports
`authoritative: false`. The protected branch must acquire a human-reviewed judge
ABI before it can prepare a supported plan; the candidate ABI is not a fallback.
