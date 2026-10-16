# Test-first red proof

Product changes require tests first. After committing tests separately from
implementation (or completing `make duel`), run `make red` at the final commit,
then `make ship`. The red runner uses the same protected judge launcher as ship.
Before the command exists on `origin/dev`, `make red-checkout` explicitly builds
and runs the checkout's binary in the pinned Docker tool image for development;
that is a non-authoritative rehearsal, not the protected dev judge.

The runner resolves exact commits before running anything. For every PR-owned
tests-only commit it exports that commit: the parent's implementation plus the
changed tests. It runs the selected packages/specs there, then exports and checks
HEAD with the same tests. At least one named failure must belong to a changed
test file, every selected check must pass at HEAD, and every red failure used
as evidence must run and pass there. Changed test files must remain byte-identical
at HEAD. Mixed product/test commits and tests added only after implementation
are refused. A timeout, cancellation, build error or nameless crash cannot
substitute for a named red test in this command; the duel's missing-API build
failure support does not itself grant a shipping record.

Rust packages, WASM suites and exact Playwright spec filters reuse the duel's
focused tooling. Browser checks use `aoeworld/browser-tools:1.63.0`, the same
image as `make test-e2e`, with the normal configured projects and disposable
server. Browser fixture changes without a spec select the existing broader
focused fixture scope. There is no `DUEL_CHECK` override for red records.
Each red or green Docker run, including its builds, has a 30-minute deadline;
Playwright retains its 20-minute suite and per-test bounds. Checks run serially
under the ship lock and one heavy slot. Output remains bounded by the focused
runner's 16 MiB total / 1 MiB line JSON limits.

The harness takes fresh runner output, never `.cache/duel/check.json`, a saved
`scope.json`, an agent's report or an uploaded evidence file. Exported inputs and
fresh captured JSON live under `.cache/agent-hook/red/`, which the existing
policy forbids agents from editing. A record is written only after both actual
runs succeed in proving red then green, using an atomic rename under
`<git common dir>/aoe-ship/red/<head>.json`. Its `head`, `base`, each `tests`
commit, `parent`, changed `paths` and named `failed` tests are recomputed or
checked by shipping. `RED_RECORD <json>` prints the record for a report.

`make ship` blocks product/build changes without the matching record before
gates/review/publication, and checks again after the final catch-up rebase.
Records are never carried across amendments or rebases, even for the same patch:
rerun `make red` for those exact new identities. Test-first history and red
evidence are included in reviewer facts and the PR description.

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
