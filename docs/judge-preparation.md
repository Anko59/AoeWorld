# Protected-policy preparation

This is preparation, not a trusted executor. `policy-prepare` resolves the
protected `dev` policy, exports its complete immutable tracked source closure,
selects protected gates, and checks protected image pins. It executes no gate.
Even a `PREPARED_NON_AUTHORITATIVE` descriptor is always `authoritative: false`.
Required hooks, preflight, coverage floors and CI gates are unchanged.

## Source and closure

Supply an existing external regular anchor JSON, outside the candidate checkout
and evidence output; symlink ancestors and hardlink aliases reject:

```json
{"schema":1,"repository":"Anko59/AoeWorld","repository_id":1374375254,"remote_url":"https://github.com/Anko59/AoeWorld.git","integration_branch":"dev"}
```

The resolver observes GitHub numeric repository identity, full name, protected
branch full commit ID, strict status-check protection and required `required`
context. A fresh credentialless HTTPS `ls-remote` must agree with the API OID.
Changed base, unavailable credentials/protection, malformed identity or network
failure returns nonzero with retained `UNAVAILABLE`, not a local-ref fallback.
Remote fetch uses that fixed full OID in a fresh external private repository.
Git network commands run under `env -i`, private HOME, disabled global/system
config, replacement objects, credentials/helpers, templates/hooks, file/ext
protocols and HTTP redirects. No candidate Git/index/ref/config/HEAD writes.

The fresh repository uses its own index, ignoring inherited `GIT_INDEX_FILE`.
Raw snapshot export binds commit/tree bytes and all tracked regular files, raw
blob IDs/bytes and modes. The complete ordered inventory rejects extra inputs
including untracked `.cargo/config.toml`, symlinks, special files, hardlink aliases,
missing files, executable-mode changes and modifications. Separate Snapshot
metadata seals are checked before any private Git verification. Whole-repository
closure includes workspace dependencies, build scripts, lockfiles, registry,
Make, workflows, instruction files and future tracked surfaces, not a handpicked
harness directory. Domain-separated BLAKE3 excludes only observation timestamp
from semantic identity. Bundle consistency is never source authentication.

## Setup

Prepare an owned external evidence directory and external anchor. Never provide
GitHub credentials to this command or store tokens in the anchor. The Make facade
canonicalizes host paths and rejects missing inputs, anchor aliases, or evidence
paths overlapping checkout/shared Git metadata, separately mounted caches, or
host runtime trees. It rejects evidence paths containing the default, `DOCKER_HOST`,
or active-context Docker socket before building/mounting. These facades require a
local Unix Docker endpoint; remote TCP/SSH contexts are refused because bind sources
resolve on the daemon host. Custom cache sources must already exist for canonical
checks. The candidate bootstrap runs with checkout/Git metadata read-only and no
forwarded Docker socket or host tokens.
API calls are unauthenticated and local image inspection cannot reach a daemon, so
either may return `UNAVAILABLE`; this is intentional until a protected, separately
trusted supervisor exists. The candidate build still executes Cargo build scripts and the
policy container has outbound network access. This is a local developer tool, not
an isolation boundary: do not invoke it on untrusted code in a secret-bearing
workspace. The coordinator's GitHub CLI package is pinned, but is not an
authenticated protected judge binary.

```sh
make orchestrator-tools
HARNESS_POLICY_ANCHOR=/absolute/external/anchor.json \
HARNESS_EVIDENCE_DIR=/absolute/external/evidence \
HARNESS_CANDIDATE=<full-lowercase-commit-OID> \
HARNESS_CADENCE=pr make policy-prepare
```

Direct CLI invocation bypasses the Make facade's read-only mounts and token
omission. Only run a trusted bootstrap with no sensitive caller environment; an
executable cannot sandbox itself.

```sh
aoe-harness policy-prepare --anchor /absolute/external/anchor.json \
  --candidate <full-lowercase-commit-OID> --cadence pr \
  --output /absolute/external/evidence
```

No `--trusted`, policy OID, local policy root, executable or observation JSON
override exists. Input/evidence overlap and unresolved-symlink checks happen before
output mutation; for disjoint paths, reused outputs receive pending `UNAVAILABLE`
before validation, and later preparation errors replace stale ready descriptors.
Invalid or unsafe output locations cannot receive an error descriptor. Remote process
receipts retain bounded stdout/stderr atomically mode0600; truncation fails.
`closure.json` binds the protected source; `preparation.json` binds candidate OID,
protected identity, registry fingerprint, complete plan and image pins/reasons.
A cancellation observed before publication downgrades ready preparation.

## ABI migration and complete selection

Protected inventory is checked for `gates/judge.json` BEFORE registry v2 parsing.
Current protected `dev` predates that ABI: explicit `UNAVAILABLE`, human-reviewed
migration required. Candidate schema/code never substitutes for protected policy.
Do not merge automatically or amend protected branch settings to enable it.

ABI schema1/ABI1/registry-schema2 accepts only six validation operations:
`fmt-check`, `structure-check`, `architecture-check`, `docs-check`, `lint`,
`test-unit`. Dispatch gate equals operation; publication/arbitrary commands and
unknown fields reject. Image roles use exact `ghcr.io/anko59/aoeworld/<role>` OCI
manifest `@sha256:<64 lowercase hex>` plus expected config `sha256:<64 hex>`.
Every selected gate must have supported dispatch; every declared image must be
present by pinned reference with matching inspected config ID. Missing capability
means `UNAVAILABLE`, never dropping unsupported checks. Independent-store diff
comparison is not implemented yet: conservatively select all protected suites,
including unknown/protected changes. No candidate registry/Make selection is used.

## Remaining boundary

The candidate-built bootstrap and ambient process environment are not an
authenticated launcher. Directly invoking untrusted code with credentials can
expose them; the Make facade intentionally forwards none. Same-UID malicious code
can forge external anchor/evidence files; filesystem checks and endpoints cannot
detect reverted races. Cargo build scripts still run during compilation.
Git raw export/probes remain unsupervised in wall time; ordinary network receipts
have bounded deadlines. Local raw Git helpers in the Make facade have no forwarded
GitHub tokens or Docker socket. Image inspection is an observation, not proof an
executed workload used that image. Live supported OCI images and migrated ABI
require separate qualification; parser fixtures do not claim that qualification.

Next boundary must run a protected-built or externally attested pinned artifact
under a separate supervisor UID with credential/evidence separation, immutable
candidate inputs, no workload Docker socket/host network/writable judge mounts,
and owned container leases plus bounded abnormal-exit cleanup. Protected Make
followed by `cargo run` in candidate cwd is NOT protected judging. No restricted
candidate executor, authenticated judge artifact, daemon-cleanup enforcement,
provider hook integration or authoritative verdict is implemented here.
