# Compiled QA single-link guard canary

[The native QA canary](../crates/harness/src/native/tests/qa.rs) copies the exact
raw bytes of [the production held-file reader](../crates/harness/src/qa/observation/io.rs)
into a real, isolated Git fixture. It does not replace the reader with a toy
policy, AST check, declared role, or mock filesystem result. Its module parent
supplies only the reader's mechanical `Result` alias and module declarations.
This narrow canary tests the regular-file single-link rejection guard, not the
entire QA report/schema/source-binding system.

The fixture pins its `blake3` and `nix` direct dependencies to the actual versions
in [the workspace lock](../Cargo.lock), then generates its own lock offline
before the initial fixture commit. The fixture lock is tracked and remains
unchanged during evaluation; the real workspace manifest and lock are not
modified. Generated snapshot caches and targets are ignored before initial
staging, preventing accidental embedded-repository inputs.

## Actual compiled controls

Two unchanged fixture tests consume files in an external, parent-owned temporary
directory. The positive control calls the production `Held::open`, reads known
binary bytes, and rechecks their raw BLAKE3 digest. The negative test observes
that an actual hardlinked file has two links, then asserts that the same compiled
production reader rejects it. The assertion has a fixed test identity and
`AOE-QA-HARDLINK-COMPILED-CANARY-v1` diagnostic token.

The sequence is:

1. A retained independent **Commit** snapshot must compile and pass both tests.
2. Only the fixture production reader is changed: remove its single-link
   condition exactly once, stage only that file, and restore good working bytes.
3. Capture one immutable **Index** snapshot and compile the staged bad reader.
   Its real negative test must exit unsuccessfully with the exact assertion token,
   test identity, and assertion diagnostic; missing tools, compilation failures,
   cancellation, deadlines, and monitor errors are not counted as a catch.
4. Recheck the same retained logical witness, source endpoints, raw effective
   index, working production bytes, test bytes, and fixture lock after failure.
5. Compile the unchanged good working reader as a passing control, then recheck
   the retained Index witness and original raw bytes again.

[The native helper](../crates/harness/src/native/mod.rs) supplies the unchanged
workspace/locked/no-fail-fast Nextest policy and final failure output, overriding
adversarial inherited reporter settings. Passing projections also run the same
workspace locked Cargo doctest arguments as the real TestUnit command. Both
command budgets remain 600 seconds. Offline lock generation also uses a fixed
600-second process budget; it must pass before any assertion catch is counted.

Baseline Commit, mutated Index, and working control use **three disjoint external
Cargo target directories**. This prevents these projections from borrowing one
another's compiled artifacts. Raw source witnesses do not authenticate compiler
or cache artifact provenance; successful controls do not qualify an independently
protected runtime artifact or trusted judge.

## Limits and handling

The fixture uses actual Unix link metadata under the canonical Linux Docker
validation environment. It is not original-asset, geographic-source, dedicated
hardware, browser, screenshot-meaning, journey-execution, runtime-build-binding,
independent QA, hidden-verdict, or authenticated approval qualification. It is
one targeted compiled mutant, not a replacement for the unchanged nightly
mutation campaign and its minimum count and zero missed/timeout requirements.

The process reader retains bounded raw tails (64 KiB per stream). The canary
checks one small assertion and the aggregate retained-log bound; it does not
promise that every failure from arbitrarily large campaigns fits those tails.
Legacy failure logs remain local and unsanitized, not private exclusive atomic
receipts. No automatic public upload or universal credential redaction is added.
Git environment clearing is inherited from the existing explicit-root process
helper; other credentials and PATH are not globally isolated. Endpoint checks
cannot detect every reverted hostile race or prove independent service ownership
while the coding UID controls the host daemon.

Parent validation must supply the actual Dockerized focused tests, strict lint,
structure, preflight, unchanged coverage floors, hooks, exact revision, and CI
results. Adding this source alone is not evidence that these gates have run.
