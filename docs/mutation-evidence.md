# Bounded mutation artifact observations

This first slice validates candidate-produced artifact bytes and their internal
records. It does **not** establish immutable campaign source, fresh execution,
compiled-cache provenance, an independent judge or authenticated approval.

## Unchanged scanner and unresolved execution boundary

The existing scanner still uses cargo-mutants 27.1.0, `--in-place`, the three
original files, `compare|classify|selection`, exclusion ` in client$`, 120 seconds
per mutant, and a 4500-second command budget. No selection, gate, floor, timeout,
provider, dependency or lock is weakened. It still runs in the coding worktree.

After-run revision/dirty fields still use the old Git CWD and inherited Git
context; they are nullable observations, **not source binding**. Report version 2
therefore always says `authoritative:false`, source/build identity unavailable,
`artifact_freshness:UNAVAILABLE`, `execution_binding:UNAVAILABLE`, and
`ROOT_CAUSE_NOT_ASSESSED`. A completed timestamp is a claim, not a freshness proof.
Fixed raw output may be old. Pending publication invalidation does not make those
raw artifacts fresh. Structural PASS is not a newly authenticated campaign PASS.

## Actual bounded byte observation

Only the fixed pair `outcomes.json` and `mutants.json` is observed: at most 4 MiB
each / 8 MiB aggregate. Original file descriptors remain held through parsing,
context observation and late checks. Normal ancestors, no-follow/nonblocking
opens, regular single-link files, identity correlation and bounded reads reject
symlinks, hardlinks, FIFOs/devices/directories, escape components, aliasing,
oversize, length changes, replacement and rewrite. Held ancestor directory
identities include device/inode/mode/owner/group, not mutable directory times.

File identities include device/inode/mode/length/mtime/ctime; rereads compare raw
BLAKE3 against the original bytes. Measurements describe actual retained complete
artifact bytes, never report-provided self hashes. Hostile reverted races,
whole-world atomicity and hard filesystem wall deadlines are not qualified.

Shared JSON parsing rejects duplicate keys at every depth before Value collapse:
4 MiB per input, depth 32, 200000 nodes, and 16384 entries per collection. Closed
pinned records bound names/argv/paths to 16 KiB, diffs/replacements to 64 KiB, and
phase argv to 128 entries. Source file paths must match the original fixed set.
Spans are positive ordered character-position claims; no AST/source-byte binding
is asserted. Nullable function records, empty return types, deletion replacements
and insertion spans are legitimate wire forms.

## Reconciliation, not a hidden verdict

The inventory has unique mutant names. Every descriptor must occur exactly once
in outcomes and match all typed fields; inventory diff is the only field removed
for that comparison. Require exactly one claimed successful Build+Test baseline,
complete inventory/counter coverage, finite nonnegative phase durations, ordered
unique phases, and claimed summaries consistent with pinned phase semantics.
Baseline success does not count as a mutant or toward the evaluated floor.

Known summaries are Success/CaughtMutant/MissedMutant/Unviable/Failure/Timeout.
Known process statuses are Success/Failure(code)/Timeout/Signalled(signal)/Other.
Signalled/Other are not Failure(code), and unclassified mutant failures cannot
supply complete evidence. These wire semantics follow the pinned upstream
[outcome source](https://raw.githubusercontent.com/sourcefrog/cargo-mutants/v27.1.0/src/outcome.rs)
and [process source](https://raw.githubusercontent.com/sourcefrog/cargo-mutants/v27.1.0/src/process.rs),
not qualification of a local binary or its service deployment.

All counter addition is checked. Require **at least 30 evaluated mutants**:
caught + missed + timeout, not unviable/success/generated count/baseline. Valid
complete missed/timeout records retain REGRESSION even when the command failed.
Zero missed/timeout, no non-Test success records, and actual command success are
necessary for a local structural PASS. Counts remain candidate tool claims.

## Failure and local publication

Pending INCONCLUSIVE/non-authoritative machine and human reports are attempted
before artifact/Git reads and before scanner execution. A publication failure
returns no accepted report; an unwriteable or hostile old file may remain and
must not be treated as authority. Artifact input/schema/
late-change/context failures are separate from original typed command outcomes.
Start/Exit/Deadline/Cancelled/Monitor/unclassified failure classes are retained
without pretending legacy errors contain actual capture duration or truncation.
The original command error is not replaced by a secondary publication failure.

Human summary precedes the final machine report. Publication uses checked regular
file descriptors and refuses symlink/hardlink targets; failed final writes attempt
pending invalidation. It remains local, non-atomic, not a protected controller
publisher; failed or hostile publication cannot grant authority. Legacy command
failure strings may contain sensitive text. No universal redaction/public upload
is introduced, and raw logs are not authenticated/private/exclusive/atomic evidence.

Parent validation must run the actual Docker focused/full gates and hooks. Unit
artifact fixtures and arithmetic tables are not a new >=30-mutant tool campaign.
Constructed legacy process-error cases prove safe class mapping and separation,
not actual runtime supervision or monitor qualification.
Next: one immutable selected Snapshot, fresh external output, retained source
witnesses, pinned tool/private-Git compatibility, then the actual unchanged nightly.
Coding UID controls Docker; local hashes, ownership, signatures and role JSON
cannot establish independent hidden-judge/controller authority. Licensed original
assets/geodata/hardware and runtime/semantic QA qualification remain unavailable.
