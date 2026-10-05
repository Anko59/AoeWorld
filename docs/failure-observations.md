# Conservative process failure observations

The crate-private `safe_observation(&Captured)` adapter describes an actual
bounded capture without returning its raw output or guessing its root cause.
The typed value is serializable, not deserializable admission/review input.
It needs no filesystem access, subprocess, source lookup, network or logfile.

## Recorded facts

Schema 1 always records `ROOT_CAUSE_NOT_ASSESSED`, including successful exits.
Outcomes distinguish success, failed exit with its optional numeric code,
deadline, cancellation, start failure and monitor failure. A failed numeric
code is `EXIT_CODE`; absence of a code is `SIGNAL_OR_UNKNOWN`, not a proven
specific signal, code defect or infrastructure diagnosis. Start/monitor errors
use an explicit fixed error-kind vocabulary, with `OTHER_UNKNOWN` for unmapped
or future kinds. The original error message and pathname are never copied.

Each stream contains its **retained tail** byte length and raw BLAKE3 digest.
These are not the complete emitted stream length/digest, compiled artifact
provenance, source identity, runtime qualification, authentication or approval.
Empty tails have measured zero length and the actual empty-byte digest.
Hashes do not make secret output harmless, nor promise universal redaction.

`truncated` preserves the capturer's aggregate flag: omitted prefix on either
stream **or** unfinished EOF/drain on either stream. It may be true with small
or empty retained tails. No invented per-stream truncation provenance is added.
Retention remains 64 KiB **per stream**, not one shared 64-KiB allowance.
`duration_ms` is actual elapsed entry/admission/capture/final-drain time rounded
down to milliseconds, not child CPU time, only workload duration, or proof of
an exact end-to-end filesystem wall bound. Final drain remains bounded to the
existing 200 milliseconds; subprocess budgets and cleanup behavior are unchanged.

No raw stream text, command/program/arguments, environment, error message or
legacy logfile path is returned. Panic-looking strings never select a code-bug
classification, severity, remediation or retry. The helper neither retries nor
suppresses earlier observations when an independent later command succeeds.

## Actual transport caller

The read-only supervisor Docker transport derives its existing `exit`,
`duration_ms`, `truncated`, `stdout_blake3` and `stderr_blake3` compatibility
fields from this same typed observation and also returns `capture_observation`.
Existing exit labels remain `SUCCESS`, `FAILED`, `DEADLINE`, `CANCELLED`,
`START_UNAVAILABLE` and `MONITOR_UNAVAILABLE`.

Raw bytes remain local for the existing successful JSON ID and printable-stderr
validation. The fixed endpoint/program/arguments, explicit cleared transport
environment, root cwd, normal ancestor and endpoint correlation guards, five
second command budget, and **4096-byte per-stream response acceptance** stay
unchanged. Response acceptance is distinct from the 65536-byte retained-tail
cap. `UNAVAILABLE`/`PROBED_NON_AUTHORITATIVE`, `authoritative:false` and
`admission_granted:false` are preserved. This adds no mutating Docker action,
worker admission or protected policy permission. Returning the already validated
daemon ID elsewhere is unchanged; the whole transport is not a universal
privacy/redaction boundary.

## Regression evidence and limits

Tests execute literal fixture commands for success, exit 7, a missing executable,
self-signalled child, zero-budget/pre-cancel no-spawn, deadline and active
cancellation with partial streams. Secret/panic-looking text on both streams
must not appear in serialized observations or become a guessed root cause.
A later successful capture cannot change the earlier failure observation.
Actual two-stream overflow and the existing escaped pipe-holder test retain
aggregate truncation and measured tail lengths/hashes even without reaching the
cap. Constructed monitor/error-kind cases are **mapping-only synthetic tests**,
not qualification of a real monitor runtime failure. Parent gates must validate
all these tests; their presence is not a claim that validation already passed.

Legacy `ProcessError` lacks actual duration/truncation/capture metadata. This
adapter does not fabricate it from timeout seconds or read an attacker-controlled
logfile. Legacy raw logs remain unsanitized, nonprivate, nonexclusive and
non-atomic; no automatic public upload or retention redesign is introduced.
General explicit-root capture clears Git variables, not every credential/PATH
variable; only the fixed transport applies its existing full environment clear.
Process-group termination does not prove cleanup of escaped sessions or daemon
containers. CI attached-head checks may execute a synthetic merge and do not
bind source/runtime identity. Local root metadata, hashes, signatures or role
JSON cannot establish independent authority when the coding UID controls Docker.
Qualified workers/controller/watchdog/hidden verdicts, protected publication,
external durable journal, authenticated approval, semantic QA, licensed original
assets/geodata and dedicated hardware qualification remain separate work.
