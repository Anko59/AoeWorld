# Immutable mutation source and local execution observations

The nightly driver selects **one retained Snapshot** and never runs the scanner in
an ambient coding worktree. No arguments selects the repository HEAD commit:
resolve it once using the same cleared Git context as scopes, prepare an independent
Commit snapshot, and reject a HEAD race at admission. An explicit `--revision`
accepts only a full lowercase SHA-1/SHA-256 object ID, including an older commit.
Both Commit modes ignore inherited alternate indexes. `--intentional-index` is
exclusive with a revision: capture the validated effective hook index once through
normal Snapshot admission. Restaging makes that retained subject stale; a new run
requires a new snapshot. There is no Working fallback.

The source probe for Commit/Index deliberately does **not** protect original
unstaged working bytes or claim that the original working tree is clean. The
selected tracked export, source HEAD/index/raw objects, and private Git path set,
types and bytes are checked before and after execution, even failed execution.
Private Git sealing includes its index and added files; it is not a global
inode/mode/owner continuity guarantee. Full logical ContentWitness values from the
same retained snapshot are compared before/after and regenerated at publication.
Compact witness summaries in the report are local measurements, not authenticated
review, independent execution or compiled binary/cache provenance.

## Fixed execution, fresh disjoint storage

The scanner remains cargo-mutants **27.1.0**, in-place, 120 seconds per mutant,
4500-second command deadline, the same three critical files, function filter and
client exclusion. The only output change is a fresh absolute external directory;
there is no fallback to an old coding-worktree campaign. A second retained private
temporary directory is a fresh scanner Cargo target. Both roots must be normal,
mode-private, and bidirectionally disjoint from source/export/Git/common-Git,
cache/Cargo home/current target/default target roots and each other. A TMPDIR inside
an input namespace fails admission. Allocation identity is rechecked; these local
0700 directories do not create independent worker authority.

The explicit-root process runner clears inherited Git variables, then reinstalls
fixed no-system/global-null/no-attributes/no-optional-locks/no-replacement and
fsmonitor-disabled Git settings for scanner children. `--cargo-arg=--locked` guards
all child Build/Test invocations; offline Cargo rejects missing cached dependencies
rather than silently downloading or accepting a rewritten source lock. Metadata
operations can still expose incompatibility: any lock/export/private-Git endpoint
change rejects the observation. Cargo home/PATH and general credentials are not a
sandbox; offline operation is not universal redaction or network confinement.

A mandatory actual `mutation-integrity` precondition runs pinned-tool strong and
intentionally weak **temporary-only** fixtures. It exercises real mutation loops
and retained Snapshot restoration, including an actual failed mutator. Tiny
canaries do not satisfy the critical campaign's **at least 30 evaluated** mutants
(caught + missed + timeout, checked arithmetic), zero missed and zero timeouts.
Real repository tests, scanner selection, budgets and coverage gates are unchanged.

## Separate failures and bounded publication

The original typed process result is stored outside the checked Snapshot closure;
postcondition failure cannot overwrite it. Report v3 separately records command
classification (including NOT_STARTED), immutable endpoints, artifact parsing/
changes, preparation and publication failures. Root cause is always NOT ASSESSED.
The command's original typed error takes precedence when returning, while every
source/artifact failure forbids PASS. A valid missed/timeout campaign remains a
regression even if the scanner exits nonzero.

The same original bounded artifact file descriptors remain held through source
checks, human-then-machine publication and final rechecks. Existing duplicate/schema,
4 MiB per-file/8 MiB aggregate, descriptor/counter/baseline reconciliation, symlink,
hardlink, FIFO and late-rewrite guards remain intact. Pending INCONCLUSIVE output
precedes fallible admission/canary/execution work. Publication is checked but local,
non-atomic and unprotected: an unwritable hostile old file cannot always be erased,
and races restored between endpoints or after return are not prevented.

Freshness means local allocation and a fixed invocation **attempt**, not tool
execution on Start/Monitor failure or authenticated candidate output. Retained raw
and target directories are destroyed after the observation; report hashes do not
promise enduring raw artifacts. Legacy process log paths may become unavailable
when snapshots are dropped. Raw local command diagnostics may contain credentials;
there is no public log upload or universal sanitization guarantee.

The coding UID controls Docker. Source equality, tool claims, local hashes, modes
and role JSON do not qualify an independent controller, hidden judge, protected
publisher, authenticated approval, external journal or served-build binding. Runtime
and semantic QA, original assets and dedicated hardware remain separate work.
See [mutation tooling](mutation-tooling.md) for the image prerequisite repair.
