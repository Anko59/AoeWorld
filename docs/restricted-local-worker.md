# Closed local worker lifecycle

`gate-run --backend restricted-local` selects an actual one-shot Docker worker,
not candidate Make. `bootstrap-local` remains the default. Restricted mode requires
an isolated Index or Commit scope; Working and unsupported operations are
UNAVAILABLE without a fallback. The six operations are fmt-check, structure-check,
architecture-check, docs-check, lint and test-unit. Source policy selection is local
feedback, not independently protected policy admission.

## Deployment prerequisite, not installation

The fixed Unix transport is `/usr/bin/docker`,
`unix:///run/aoeworld-supervisor/docker.sock`, with client configuration at
`/etc/aoeworld/supervisor/docker-client`. The fixed template is
`/etc/aoeworld/supervisor/worker-template.json`. It requires schema 1, full immutable
image configuration ID (`sha256:` plus 64 lowercase hex), nonroot uid, memory_mib,
pids, cpus, workload_s, cleanup_s, command_s, tmp_mib and target_mib. Unknown,
duplicate and missing fields are rejected. No endpoint, program, argv, role,
signature or caller-selected mount is accepted. No image pull/build/import occurs.
Root-owned single-linked template mode 0600 and protected ancestors are hygiene.
An ordinary coding UID may not read that template: absence or denial is UNAVAILABLE,
not a request to change users, privileges, ownership or install root services.

A deployment-prebuilt judge image must have User `<uid>:<uid>`, WorkingDir
`/candidate`, Entrypoint `["/judge/aoe-harness"]`, empty Cmd, and no image volumes,
labels, on-build hooks, healthcheck or exposed ports. Exact seven unique Env key/value pairs (ordering is immaterial):
`PATH=/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/bin`, `HOME=/scratch`, `LANG=C`,
`LC_ALL=C`, `CARGO_HOME=/opt/cargo`, `CARGO_NET_OFFLINE=true`,
`CARGO_TARGET_DIR=/target`. Offline dependencies must already be present in the
read-only image. Candidate Cargo/build scripts/tests execute only inside the
worker; they do not build the controller/judge or authenticate executed bytes.
Nested Docker and absent offline dependencies fail honestly; no socket is added.

The worker uses read-only root/source, network none, all capabilities dropped,
no-new-privileges, explicit nonroot uid/gid, private IPC/cgroup namespaces,
memory/swap/pids/CPU limits, and bounded fresh tmpfs scratch/tmp/target mounts.
Target is executable for build scripts/tests. No shared trusted writable caches,
host devices/namespaces or Docker socket are mounted. Actual inspect configuration
must match, not just labels claiming those settings.

## Measured lifecycle and limits

Fresh local nonce journal intent is synced before create; full observed CID is
persisted before start. Numeric Docker-wait output is checked against stopped
container state and stays separate from the Docker CLI capture outcome.
Docker-logs CLI tails are bounded transport observations, not an invented exact
container stdout/stderr transcript or workload duration. Cleanup has a separate
reserve, independent of cancellation: exact-ID enumeration/inspection, stop/kill
when needed, remove and successful absence enumeration. Ambiguous create uses
only the current internally generated nonce, never a journal supplied by a caller.
Multiple IDs or inconsistent configuration quarantine without global deletion.
Template/source/journal changes block further workload success; original binding
cleanup remains possible only while the fixed transport remains unchanged.

Raw local logs/ledger remain UNSANITIZED. Safe output contains typed fresh facts,
not raw paths, daemon messages or worker-supplied authority. Source witnesses,
transport captures, exit, cleanup, journal, retention and final publication are
independent planes. Failed publication never reads old PASS bytes as fallback.
No restart recovery or watchdog surviving SIGKILL is implemented: old journal
records are retained correlation data, never cleanup authority. No claim covers
arbitrary escaped descendants, kernel isolation qualification, reverted endpoint
races, physical power-loss durability or hard wall supervision of filesystem IO.

All results remain non-authoritative. Coding UID ownership of Docker invalidates
promotion based on root UID, modes, labels, hashes or local signatures. Independent
controller/service authentication, protected policy/key/image configuration and
installation roots, hidden judge and publisher require separate external setup.
Their absence means authoritative results UNAVAILABLE even after local exit zero.
Six-operation execution does not qualify semantic/browser QA, assets or hardware.

## Explicit actual canary request

The canonical `local_canary_request_is_explicit_and_never_asserts_authority` test
performs no Docker execution when both request variables are absent and explicitly
reports UNOBSERVED. This request-contract branch is not Docker qualification.
For actual local fixtures, a parent/operator supplies `AOE_WORKER_CANARY_IMAGE`
(full immutable configuration ID) and `AOE_WORKER_CANARY_ROOT` (existing canonical
owned directory mounted at the same absolute path in outer test container and
Docker-daemon host). This private cfg(test) adapter uses `/usr/bin/docker` at fixed
`unix:///var/run/docker.sock`; it cannot alter the production endpoint or template.
It allocates fresh disjoint source/output/client roots and retains real journals.

A purpose-built canary image obeys the image contract above at uid 65532. Its fixed
operation behavior is: fmt-check verifies actual nonroot/capability/network/rootfs
and source-write denial before emitting `AOE_LOCAL_SANDBOX_CANARY_V1`; structure-check
exits 7; architecture-check emits more than 64 KiB on each stream; docs-check sleeps
30 seconds; lint/test-unit exit zero with the marker. Fixture target deadline is
2 seconds; a second docs scenario cancels when the real Wait action begins after
Start. Every requested scenario requires observed exact-CID absence and unchanged
source sentinel. Mock traces only test wiring. Actual canaries remain explicitly
local and non-authoritative, never protected judge/RTS semantic qualification.
