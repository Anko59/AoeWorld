# Bounded native failure output and a real assertion canary

The canonical native test command now calls the fixed Rust policy in
[native/mod.rs](../crates/harness/src/native/mod.rs). It executes Cargo Nextest
with the same workspace, locked inputs, no-fail-fast behavior and **600-second**
deadline. Its reporter arguments are fixed:

```text
nextest run --workspace --locked --no-fail-fast --failure-output final --success-output never
```

Final failure output puts a bounded failed-test assertion block after ordinary
status noise. Explicit arguments override inherited Nextest reporter settings.
They do not filter tests, add retries, ignore failures, change status verbosity or
increase capture limits. The following Cargo workspace doctest command and its
600-second deadline remain unchanged.

## Actual integrity canary

[The native tests](../crates/harness/src/native/tests.rs) create a temporary real
Git repository with a no-dependency Rust workspace and fixed offline lockfile.
They run the **same native policy helper** against an immutable passing commit
before testing a mutation. A success-output sentinel is checked with the same
fixed arguments and budget, including an adversarial local reporter environment.
No missing tool or unsuccessful baseline counts as a canary catch.

The fixture stages only a valid, compiling production mutation: a marker returns
`2` instead of `1`. The working copy still returns `1`, and assertion test bytes
are unchanged. One captured index snapshot is retained through execution and its
logical raw-content and original endpoint checks. Cargo runs locked and offline,
with disjoint external target directories for the passing commit, mutated index
and working control. This avoids cross-projection build-cache reuse; a source
witness by itself does not certify the provenance of cached compiled artifacts.

The staged test emits over 64 KiB of synthetic noise before its known assertion.
The canary requires an actual nonzero native exit, the fixed test name and token,
and the assertion diagnostic in the retained bounded failure log. A compilation
error, timeout, unavailable tool or exit code alone is not an integrity catch.
It also checks the capture truncation flag and both stream bounds, unchanged
source index bytes, working source, test and lockfile bytes, and the same logical
snapshot witness before and after the command. The unchanged working source runs
as a final passing control.

## Evidence and authority limits

The process capture remains **64 KiB per stream**, retaining raw tails. Its
aggregate truncation flag can indicate an omitted prefix or incomplete final
pipe drainage. A retained-tail hash would not certify the complete original
stream. Final output does **not** guarantee that arbitrarily many or oversized
failure blocks fit in those limits. The canary demonstrates one controlled
assertion mutation, not full semantic mutation coverage or hidden-test security.

The explicit-root process helper clears Git environment variables; other inherited
environment values, credentials and PATH are **not globally isolated**. Existing
process cancellation, deadlines, read quotas and tail tests remain separate.
Legacy local failure logs contain raw untrusted output and have no authenticated
review role, private atomic-storage guarantee or universal redaction. They must
not be blindly uploaded as public CI artifacts. Cargo exit `100` alone still has
an unassessed cause unless actual diagnostics establish more.

The legacy working-source preflight is not upgraded here to one retained immutable
snapshot across its entire native phase. This canary uses one checked immutable
snapshot, but local process-group termination does not establish daemon-container
ownership, escaped-descendant cleanup or a qualified deployment. Coding-UID host
daemon access still prevents local hashes, signatures or root metadata from
creating independent judge authority. Original asset, geographic-source and
hardware qualification, broader property/mutation checks and QA triage remain
separate work. The full harness objective is not complete.
