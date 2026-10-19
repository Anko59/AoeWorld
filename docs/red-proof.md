# Test-first red proof

Product changes require tests that fail without them. At the final commit run
`make red`, then `make ship`. The red runner uses the same protected judge launcher as ship.
Before the command exists on `origin/dev`, `make red-checkout` explicitly builds
and runs the checkout's binary in the pinned Docker tool image for development;
that is a non-authoritative rehearsal, not the protected dev judge.

The proof is of the whole pull request, not of its commits. Commit however you
like (tests first, mixed product and test commits, one commit): the harness
looks only at the merge base with the selected base and at HEAD.

- **Changed tests** are the tests the PR's whole diff adds or changes: Rust test
  functions by qualified name (bodies compared, any attribute stack), Playwright
  specs by title and location, and specs importing a changed helper (including
  transitive static relative imports).
- **Red**: the runner exports the merge base, lays ONLY the PR's test-file
  changes over it (product code exactly as before the PR, except the `mod`
  lines below) and runs the selected
  packages/specs in that snapshot. It is red when at least one changed test
  fails by name, or when a changed Rust test target or Playwright spec cannot
  compile/load because it names an item the PR adds (see below).
- **Mount lines**: a Rust test file is compiled only where a `mod` item loads it,
  and that item usually lives in a product file. For each changed Rust test file
  that `head` mounts from a product file the merge base does not declare it in,
  the red tree gets that one declaration (`#[cfg(test)] mod tests;` or the
  `#[path]` form) appended to the base's file, nothing else of the product. A
  parent module the base lacks becomes a stub holding only the declaration of its
  child, itself mounted in turn, so the tests of a new module fail to compile
  against the missing item (compile-red). For a test file the PR deletes, the
  declaration the product parent dropped is removed from the red tree, so the
  build does not stop at E0583. When the PR moved an inline `#[cfg(test)] mod
  name { .. }` into its own file (the test-placement hook forces that move), the
  base's inline block of that name is removed from the red tree, so the module is
  defined once (no E0428); only a `cfg(test)` block is removed. Declarations inside
  inline `mod x { .. }` blocks are not followed (limit).
- **Pre-existing failures never count**: the old test files are run at the merge
  base too, and any name failing there is removed from the evidence. Changed
  tests that all pass at the merge base, or already fail there, are refused.
- **Green**: the same selection passes at HEAD, every red failure having run
  and passed there. After a compile-red, at least one changed test of every file
  that failed to compile must have run and passed at HEAD (a filtered, ignored,
  cfg-gated or unmounted file is refused), and the record's `ran` names them.
- **Attributes**: red counts a test as changed whatever its `#[ignore]`/`cfg`
  stack, because only a test that ran can fail by name and green shows what ran.
  The duel's own green keeps its exemption for ignored and cfg-gated tests.

## Compile-red

A Rust test that calls an API the PR adds cannot compile at the merge base. That
is accepted as red only when every compiler error is located in a changed test
file and at least one is a "cannot find" kind
(`E0405 E0412 E0422 E0423 E0425 E0432 E0433 E0531 E0560 E0599 E0609`) naming an
identifier that occurs on a line the PR adds to non-test code. The other errors in
those files are its cascade (E0282/E0283 "type annotations needed" and method
errors on values built from the missing items); each such file still needs a test
that runs and passes at HEAD. The record names the missing items. An error
outside the changed tests, a build with no such miss, a truncated build log, a
timeout, a cancellation or a nameless crash is not red.

Playwright collection/load errors count only when every error belongs to a
changed spec or a changed helper statically imported by a selected spec
(including transitive relative imports and `.js`/`.mjs`/`.cjs` specifiers resolving
to `.ts`/`.mts`/`.cts` helpers). The fresh JSON report's top-level and
suite errors must identify a missing module (`Cannot find module`,
`Failed to load url`, `ERR_MODULE_NOT_FOUND`, or a missing `copyfile` source),
or an export (`does not provide an export named`). Each missing module must
exist at HEAD and not at the merge base; each missing export must occur on a
line the PR adds to non-test code. The record's `missing` names those paths or
exports. An accompanying `No tests found` collection summary is accepted only
with explained load errors. Unknown errors, errors outside that scope, missing
or truncated JSON, deadlines and runner failures remain unavailable/refused.
Playwright's `<anonymous>` ESM eval location needs a named source/importer in
the message or stack. A real file location remains authoritative, including
when it points outside the changed scope.
At green, every affected spec must have at least one changed test that ran and
passed; helper failures require this of each importing spec. Passing evidence
requires Playwright's `expected` outcome, `expectedStatus: passed`, and a final
result with `status: passed`. Expected failures (`test.fail()`), fixme, skipped,
filtered or unobserved tests cannot earn proof. These test names are recorded
in `ran`, as for Rust.

## Runs and records

Rust packages, WASM suites and exact Playwright spec filters reuse the duel's
focused tooling. Each exported snapshot uses separate Cargo build output and
browser dependencies so archived timestamps cannot reuse another tree's tests.
Browser checks use `aoeworld/browser-tools:1.63.0`, the same
image as `make test-e2e`, with the normal configured projects and disposable
server. Browser fixture changes without a spec select the existing broader
focused fixture scope. There is no `DUEL_CHECK` override for red records.
A `make red` does up to three runs (red, the old tests on the merge base, green), each
from a fresh export with its own cold Cargo target: sharing one target between trees
is unsafe because archived timestamps can make Cargo reuse another tree's test binary
(the reason for the separate outputs above). Measured once on a small crate
(`crates/core`, 6 CPUs per container): all three runs took 19 s. Large packages are
unmeasured, and a run past the deadline below is "red runner unavailable", never a record.
Each red or green Docker run, including its builds, has a 30-minute deadline;
Playwright retains its 20-minute suite and per-test bounds. Checks run serially
under one heavy slot (not the ship lock). Output remains bounded by the focused
runner's 16 MiB total / 1 MiB line JSON limits.

The harness takes fresh runner output, never `.cache/duel/check.json`, a saved
`scope.json`, an agent's report or an uploaded evidence file. Exported inputs and
fresh captured JSON live under `.cache/agent-hook/red/`, which the existing
policy forbids agents from editing. A record is written only after both actual
runs succeed in proving red then green, using an atomic rename under
`<git common dir>/aoe-ship/red/<head>.json`. Its `head`, `base`, changed test
`paths` with their blob ids, named `failed` tests, `missing` API items and the `ran` tests of a compile-red are
recomputed or checked by shipping. `RED_RECORD <json>` prints the record for a report.

`make ship` blocks product/build changes without the matching record before
gates/review/publication, and checks again after the final catch-up rebase.
Each record stores the review's merge-base patch fingerprint and a separate,
stronger red-carry fingerprint of a diff with 50 surrounding lines. Red ignores
hunk offsets and text blob IDs, preserving exact changed bytes,
function headers, context, paths, modes and binary identities. When the current
head has no record, shipping checks the red-carry index (at most 128 candidates),
recomputes both source identities against the recorded base, and carries only
when both match, along with changed test paths and head blobs. Distant upstream
insertions can shift line numbers without losing proof; moving an identical edit
to another function or changing its context requires `make red` again. Missing or
unreadable identities refuse carry. A carried PASS names the original head and
is included in reviewer facts and the PR description.

Product scope is non-test code under `crates/` except `crates/harness/`, plus
`web/` and `browser/`. Build-time inputs retain the shipping classifier's
Cargo/build-script rules. Harness, gates, documentation and CI alone are not
applicable. Docs, config and pure refactors can declare an exemption with a
commit trailer:

```text
Harness-Test-First: exempt — pure refactor: extract an unchanged helper
```

The exemption is allowed and reported with its commit and reason; reviewers
still reject an exemption whose reason does not hold. It is not a way to call
a behavior change a refactor.

For a local rehearsal, the CLI accepts `red --root <checkout> --base <ref>
--head <ref>`. Make forwards `HARNESS_RED_ROOT`, `HARNESS_RED_BASE` and
`HARNESS_RED_HEAD` for the same purpose. Roots and refs are resolved to exact
commits; working product/test edits do not participate.
For a stacked branch, select the same parent with `HARNESS_RED_BASE` that
shipping selects with `SHIP_BASE`.

## Threat model and limits

Tester/implementer subagents are in scope: they cannot create records by writing
evidence files, edit Git metadata or protected captured output, run `make red`
or `make red-checkout`, commit, edit harness/Make policy, or change each other's
file classes. The actual runner emits fresh observations to its supervising
harness over a captured process pipe. No command imports caller-supplied
observations into the record store.

A deliberately forging main session is out of scope. This is local workflow
enforcement, not hostile same-UID isolation or cryptographic remote attestation.
Candidate tests/build scripts execute code, caches are mutable, and contrived
interpreter attacks remain the general runtime-policy limit. Review must judge
whether a failing test establishes the intended behavior; CI reruns current
gates but does not independently reproduce historical red. Synthetic browser
fixtures do not qualify original assets, finished RTS performance or dedicated
hardware.
