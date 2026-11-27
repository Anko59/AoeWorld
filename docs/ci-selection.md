# CI selection

Owner, 2026-11-26: "I need a better test run policy, where PRs that touch different parts of the code trigger
different tests. So everything doesn't run all the time."

A pull request runs the jobs, the unit tests and the static checks its change can break, and the `select` job says why.
A push to `dev` runs everything; the nightly runs its own long jobs and does not use the selection. The code is `crates/harness/src/gates/select/`; the data is
[`gates/ci-rules.json`](../gates/ci-rules.json); `make ci-select` writes the manifest and
[testing](testing.md) describes the jobs.

## The rule

1. **A changed file maps to a crate.** The crate is the one whose directory holds the file, among the workspace
   members and the two crates outside the workspace (`crates/sdk-python`, `fuzz`). `cargo metadata` gives the
   dependency edges; nothing in the rules lists which crate depends on which.
2. **A job runs when a changed crate is in the build closure of a crate it builds or tests.** The job table in the
   rules file names those crates: `browser` (wasm tests: `aoe-client`, `aoe-rendering`, dev-dependencies included),
   `e2e` (`aoe-client`, `aoe-server`), `target-performance` (`aoe-assets`, `aoe-client`, `aoe-protocol`,
   `aoe-server`, `aoe-simulation`), `fuzz-smoke` (the fuzz targets, the Python bindings and `aoe-server`, which the bindings' tests run). A build follows normal
   and build dependencies only, so a dev-dependency cycle (the SDK's tests use the server) does not make a Rust AI
   change rebuild the Python bindings.
3. **Paths outside crates have explicit rules.** `browser/**` and `web/**` run `browser` and `e2e`;
   `baselines/**` runs `target-performance`; `ai/**` runs the static checks only; `docs/**`, `qa/**`, `reports/**`
   and Markdown inside crates are inputs some tests read; instruction files and skills are inputs of the harness
   tests; `.github/**`, `make/**`, `gates/**`, `docker/**`, the Makefile, every `Cargo.toml`, `Cargo.lock` and the
   toolchain file select everything; `deny.toml` runs the static checks that compile.
4. **A file some test reads runs that crate's tests.** A crate whose sources name the file (or the directory that
   holds it, alone or with a dynamic name) runs its unit tests, and nothing else: a golden under `browser/tests/wire`
   that a protocol test includes runs the protocol's tests and their dependents.
5. **Harness changes.** A module of `crates/harness/src` listed in the rules selects the jobs whose plumbing it holds
   (`perf*` selects `target-performance`, `e2e` selects `e2e`, `wasm_test` selects `browser`, `fuzz` and `sdk_python`
   select `fuzz-smoke`, and `review`, `agents`, `roadmap` and the like select no job). `ship` is not narrowed: its git,
   gh and JSON helpers serve `e2e`, `perf_comment` and the unit leg itself. Every other harness
   file selects everything, because the harness is the plumbing of every job.
6. **A path no rule covers selects everything.** A test checks every tracked path against the rules, so a new
   top-level path fails it until someone writes its rule.

## What each change runs

Unit scope is the changed crates plus every crate that depends on them, edges of any kind. The tests in
`gates/select/tests/table.rs` pin each row.

| Change | Jobs that run besides `static` | Jobs skipped | Unit tests | Static legs |
|---|---|---|---|---|
| docs only (a file no test reads) | none | all | none | light |
| `ai/` Python only | none | all | none | light |
| `crates/client` only | browser, e2e, target-performance | fuzz-smoke | client | checks, unit |
| `crates/rendering` only | browser, e2e, target-performance | fuzz-smoke | rendering, client, harness | checks, unit |
| `crates/simulation` only | browser, e2e, target-performance, fuzz-smoke | none | simulation, client, sdk, server, ai, harness | checks, unit |
| `crates/ai` (the Rust AI), `crates/server` | e2e, target-performance, fuzz-smoke | browser | ai (server), server, sdk, harness | checks, unit |
| `crates/protocol` | browser, e2e, target-performance, fuzz-smoke | none | protocol and every dependent | checks, unit |
| `crates/map` | browser, e2e, target-performance, fuzz-smoke | none | map and every dependent | checks, unit |
| `crates/geodata` | none | all | geodata | checks, unit |
| `crates/assets` | browser, e2e, target-performance, fuzz-smoke | none | assets, client, harness | checks, unit |
| `crates/sdk-python/src`, `fuzz/fuzz_targets` | fuzz-smoke | browser, e2e, target-performance | none (outside the workspace) | light |
| `crates/harness/src/perf_comment` | target-performance | browser, e2e, fuzz-smoke | harness | checks, unit |
| `crates/harness/src/review` | none | all | harness | checks, unit |
| `crates/harness/src/ship` | all | none | whole workspace | checks, unit |
| `.github/**`, `make/**`, `gates/**`, `Cargo.lock`, any manifest or lock file | all | none | whole workspace | checks, unit |
| `browser/tests/**` only | browser, e2e | target-performance, fuzz-smoke | harness (its tests read the directory) | light, unit |
| `deny.toml` | none | all | none | checks |

`native-coverage`, `qualification` and `exhaustive` run on pushes to `dev` only, never on a pull request.

The **static legs** are the matrix of the `static` job: `light` (`make preflight-light`: format, structure, architecture
and documentation checks, no compile), `checks` (`make preflight-checks`: those plus `deny`, the wasm build and
clippy) and `unit` (`make preflight-unit`: `test-unit` for the unit scope, then `perf-smoke` when the scope holds the
simulation, the server or the protocol, or a harness performance module changed). The workflow reads
`static_parts` and `unit_scope` from the `select` outputs; `AOE_UNIT_SCOPE` carries the scope to the leg.

## Local checks: the game score

`local_checks` in the rules file lists checks that `make ship` runs on the agent's machine and CI does not. They use
the crate graph the way a job does, so one answer to "can this change affect X" serves both, but they are not jobs:
the manifest, the `select` outputs and `required` never list them. The only one today is `game-score`
([game score](scale/game-score.md#in-make-ship)):

- It runs when a changed path is in the build closure of `aoe-server` (the simulation, the Rust AI, the map, the
  protocol, the SDK, core and scenario crates), or is an input the rule names: `Cargo.toml`, `Cargo.lock`,
  `rust-toolchain.toml`, `crates/harness/Cargo.toml` and `crates/harness/src/game_score`.
- A path under `ignore` never runs it: the tests, benchmarks, examples and notes of those crates are not part of
  any build of the server.
- The rule's `inputs` and the directories of the closure are also what the results cache hashes, file by file and
  less the files `ignore` names, so the list that decides whether the game score runs is the list that decides
  whether a stored result is still true: a change to a test or a note of a game crate neither runs the check nor
  changes the key.

`make ci-explain` prints one more line after the jobs: `game-score (make ship, not a CI job): runs because ...` or
`skipped because ...`.

## See why

The `select` job writes one line per job to its summary, "runs because aoe-client builds on aoe-simulation, which
changed (crates/simulation/src/lib.rs)" or "skipped because no changed crate is in the build closure of ...", then the
unit scope line. `make ci-explain` prints the same for the current branch against `origin/dev` (`CI_BASE=<ref>`
changes the base); it counts commits only, so commit before reading it.

## Safety net

Pushes to `dev` select everything: with no base there is no changed path, and no changed path selects
everything. `required` keeps its name and its rule: it recomputes the manifest at the checked-out revision, compares
it field by field (reasons, legs and unit scope included) and accepts a skipped job only when the manifest
deselected it. `AOE_TEST_ALL=1` forces the whole workspace in any leg, and an unreadable `AOE_UNIT_SCOPE` is an error.

## Known limits

- `perf-ci` runs `perf-wasm-size` and the instruction kernels (`perf-instructions`) together and compares both with
  their baselines, so a client-only change, which must run the wasm size check, also runs the simulation, protocol
  and asset kernels. Splitting them belongs to the performance gates.
- The registry's suite globs still drive the edit, stop, commit and preflight cadences (`make ship`); only CI's job
  selection uses the crate graph. `make ci-explain` is the truth for CI.
- The mention rule reads source text, so a path built at run time from pieces is invisible to it; the push to `dev`
  runs the whole suite and catches that.
