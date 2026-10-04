# Supervisor requirements and lease model

This slice models contracts for a future independently deployed judge. It is **not
an authenticated supervisor, restricted worker or validation verdict**. Every
output is `authoritative:false`; well-formed requirements retain `UNAVAILABLE`
admission. A zero command exit means the model report was written, not that an
artifact, deployment, cleanup or gate qualified.

## Invocation

The Rust command is:

```sh
aoe-harness supervisor-model --requirements /absolute/requirements.json \
  --output /absolute/existing-feedback-directory
```

The thin Docker facade is `make supervisor-model`. Set
`HARNESS_SUPERVISOR_DIR`, `HARNESS_SUPERVISOR_REQUIREMENTS` (inside that directory)
and `HARNESS_EVIDENCE_DIR` to existing absolute paths. Make canonicalizes these
paths and rejects evidence overlap with the checkout, common Git metadata, writable
caches, host runtime paths, Docker sockets or the supervisor input directory;
configured non-Unix Docker endpoints are rejected because bind paths resolve on the
daemon host. The model container uses network none, read-only checkout/Git/input
mounts, and no Docker socket or forwarded API credentials. This is feedback
plumbing, not proof that the coding host or model lacks daemon control. Compilation
uses the normal development Dockerized Make tools.

Requirements are strict schema1 JSON, bounded32KiB. Top-level fields are `schema`,
`anchor` (the existing protected-source anchor contract), `service_uid`,
`candidate_uid`, three disjoint absolute directories `artifact_root`,
`evidence_root`, `lease_root`, `subject`, optional `attestation`, and `resources`. Candidate UID must be
nonzero and distinct from the declared service UID. Roots must be normal existing
nonlinked directories, disjoint and outside the candidate checkout. Local root
UID/mode observations are reported, not elevated into access-control assurance.

The **unverified** subject has schema1, numeric `repository_id`, fixed
`protected_ref` (`refs/heads/dev`), full lowercase `protected_commit` and
`protected_tree`, raw `closure_blake3`, domain-qualified canonical `registry_hash`,
`executable_blake3`, `runtime_blake3`, `trust_root_id`, and the existing strict
judge `abi`. Digest fields are64 lowercase hex characters; the registry hash uses
`blake3:registry-v2-canonical-v1:`. The ABI must bind all six existing closed
operations and1..16 content-pinned image roles. No publisher, shell, verifier,
CID or `verified:true` fields are accepted. Fixture digests/images are synthetic;
copying them cannot authenticate an artifact or produce live OCI pins.

Resources constrain memory64..32768MiB, pids1..1024, cpus1..32,
workload1..3600seconds, cleanup1..15seconds and command1..5seconds; command must
fit cleanup. A nonexecuted worker template records a fixed judge entrypoint and
operation, empty environment/groups, network none, read-only root, capability
drop, no-new-privileges, no socket/host namespaces/writable judge or evidence/
shared trusted caches. These declarations are not observations of a running
container, and placing a judge binary beside hostile guest code is insufficient
runtime integrity: verdict computation and hidden tests belong outside it.

## Lease controller model

A private generic controller uses fixed typed actions and service-owned simulated
intent identities. Callers cannot submit a container ID, name, selector, cidfile
or deletion path. A full observed64-character lowercase CID is bound to service,
epoch, nonce, daemon, image-config and security correlation fields. Exact identity
is inspected before every destructive action. Labels are correlation, not
cryptographic service authentication.

Intent acknowledgment precedes create. A failed create receipt can recover only
zero or one inspected match for that intent; multiple/malformed matches quarantine.
Truncated, late or otherwise incomplete calls stay incomplete, with no claim that
an unknown daemon object was removed. Recovery after such uncertainty needs a
future durable journal/startup reconciler; the model's intent hook is not fsync.

Cleanup has a separate uncancelled reserve (max15seconds), shared across inspect,
stop, optional kill, remove and final absence inspection. Each action gets at most
5seconds and no action starts with zero remaining time. In-command and cross-command clock rollback, late exit,
truncated/oversized output, malformed identity or acknowledgments, and daemon
unavailability never become successful cleanup. Removal client exit alone is not
enough: final absence is required. Receipt status is `COMPLETED_MODEL`,
`INCOMPLETE` or `QUARANTINED`, never a gate PASS. The fixed scenarios are separate
from the user's requirements: they exercise the common algorithm against an
in-memory daemon, not a Docker adapter using those declarations. A controller
watermark checks every clock observation before and after calls and before
admission/cleanup deadline construction. Rollback cannot enlarge a reserve, and
deadline overflow does not spawn an intent. A future transport still needs actual
bounded capture and a genuinely monotonic clock; this is not wall supervision.

The model tests explicitly preserve a daemon object after client/controller loss.
They do not prove actual process-group signals or daemon crash recovery. They
cover ownership substitutions, shared deadline/cleanup reserves, lost receipts,
ambiguous recovery, partial replies and removal that lies about absence.

## Remaining deployment boundary

The optional [signature observer](artifact-signatures.md) implements strict Ed25519,
but does not authenticate an external trust domain or inspect executable/runtime
bytes. There is no protected-built executable/runtime admission, qualified deployment,
real Docker transport, durable lease journal/watchdog or restricted execution.
Protected dev's missing ABI still requires human-reviewed migration; no candidate
fallback or made-up image pins. The coding UID currently controls the host daemon,
so a second UID, root-owned file, local hash or socket-group adjustment on this
host cannot establish authority against that model. Qualification must be provided
outside that control domain.

Future Docker transport must clear **all** inherited environment, use fixed
service-owned executable/HOME/config/daemon identity, bounded actual subprocess
capture, and independently reserved cleanup. Existing feedback capture clears
Git/Make variables but is not credential isolation. Killing a Docker client process
group does not remove daemon containers. SIGKILL/power loss requires externally
managed durable recovery; there is no unconditional cleanup promise when the daemon
is unreachable. Evidence remains incomplete and new work must stop when cleanup
debt exceeds its bound.

The atomic0600 feedback report replaces stale readiness on inner errors. Input
aliases into the output directory are rejected before any replacement to avoid
clobbering caller input. Any nonzero invocation establishes no fresh evidence from
an older retained report. Endpoint checks are same-user observations and miss
reverted races; filesystem reads are not wall-supervised. No role, SDK hook,
hidden-test confidentiality, publication, merge or hardware qualification is granted.
