# Supervisor IO primitives — not qualified execution

The model command supports two opt-in primitives. Neither creates a trusted role,
owned real container, approval, verdict, protected build or deployment authority.
The overall model report is always `UNAVAILABLE` and `authoritative:false`.

```sh
aoe-harness supervisor-model --requirements /external/requirements.json \
  --output /external/private-feedback --persist-model-journal --probe-service

make supervisor-model HARNESS_SUPERVISOR_PERSIST_JOURNAL=1 \
  HARNESS_SUPERVISOR_PROBE_SERVICE=1
```

Without those options no journal or daemon probe is requested. The Make runtime
continues to have no network or socket mounts and no API tokens: probing inside
it normally remains unavailable. The flags do not expose a daemon, custom program,
CID, command, credentials, key path or publisher selection.

## Credential-cleared read-only probe

Production can execute only this fixed command, never a mutating Docker action:

```text
/usr/bin/docker --host unix:///run/aoeworld-supervisor/docker.sock \
  --config /etc/aoeworld/supervisor/docker-client info --format '{{json .ID}}'
```

Before and after capture, fixed path observations must agree. The program is
single-linked, regular, root-owned, executable and not group/world writable;
ancestors are nonlinked root-owned directories without group/world write. The
Unix socket must be root-owned, single-linked and0600. The root-owned client
configuration directory permits only an optional root0600 single-linked bounded
4096byte `config.json` containing `{}`: credentials, helpers, contexts, extra files
and alternate endpoints do not select a fallback.

The child clears **all** inherited environment variables. Only fixed HOME
`/var/empty`, PATH `/usr/bin:/bin`, LANG `C` and LC_ALL `C` remain; cwd is `/` and
stdin is null. The existing real process supervisor owns the child process group,
checks cancellation/deadline, drains both streams fairly and captures bounded tails.
The probe allows five seconds and rejects truncated or over4096byte responses on
either stream, unsuccessful exits, invalid encoding/control data and malformed,
empty or over256byte daemon IDs. IDs are correlation observations, not server or
peer authentication. Reports use bounded exit categories and byte fingerprints,
not raw errors or stderr dumps. SIGINT/SIGTERM observation is scoped to the probe
CLI; cancellation prevents subsequent journal work and retains unavailable feedback.

Actual private executable fixtures test inherited GH/GITHUB/CARGO/AWS/Git/Docker,
PATH and loader-variable removal in a separately spawned test process; no unsafe
global environment mutation is used. Deadline, cancellation, failure, partial,
binary and oversized stream fixtures exercise the real capture primitive. They do
not contact or qualify a real daemon. Filesystem/path observation itself is not
wall supervised; root metadata is not executable measurement or service authority.
Killing a Docker client process group does not remove daemon-owned containers.

## Durable local model journal

`--persist-model-journal` uses only `lease-journal` inside the already validated
private external feedback directory, never a caller record/CID/deletion path.
Parent/root directories must be owner0700 and nonlinked. Directory descriptors
anchor open/rename operations; handle/path owner/mode/inode identity is rechecked.
The fixed owner0600 single-linked `lock` uses a nonblocking exclusive kernel flock,
held across history read, append, sync and endpoint checks. Lock/path replacement
rejects rather than admitting another history.

The strict schema1 ledger has `authoritative:false`, mode `MODEL_ONLY` and at most
32entries/32KiB. Sequence, paired fixed model IDs, simulated identity, phase,
previous hash and domain-qualified BLAKE3 entry hash must agree. Intent has no CID;
a model observation records only a fixed simulated CID. Deserialized data is
untrusted correlation, not an `OwnedLease`. Corrupt, malformed, linked, oversized,
wrong-owner/mode, exhausted or inconsistent history rejects without resetting it.

Each append creates an exclusive no-follow owner0600 temporary file, writes all
bytes and syncs the file. Before publication it re-reads the bounded temporary
handle, compares exact bytes and synced handle/path metadata (including inode,
mode, owner, single-link count and change times), then rechecks the expected history.
Only a matching prepared temporary is renamed relative to the held directory
descriptor, synced through the directory and reopened. These pre-publication
rejections retain the previous history and temporary debt. This is best-effort
endpoint correlation, not protection against a hostile same-UID swap after the last
check or an authenticated publication primitive.
Creation of the journal directory is followed by parent directory sync. Reuse
appends another model pair or resumes one already-persisted odd intent with the
same correlation ID. Failed temporary files are retained as debt; no broad cleanup
or claim that unknown files or daemon objects were removed is made.

Real filesystem tests cover reopens, persisted intent reuse, cooperating writer
exclusion, changed expected heads, corruption/budgets, symlinks/hardlinks,
permissions and directory/lock replacement. `fsync_calls_completed:true` means
those calls returned, not power-loss, storage firmware or production durability
qualification. Journal reads/syncs are not deadline bounded.

## Still missing

These primitives are not connected to authenticated worker creation, daemon lease
ownership or destructive cleanup. The controller's old in-memory persistence hook
is still not a durable service write. No daemon startup/SIGKILL watchdog or hidden
test execution exists. Consistent hostile history rollback/rewrite cannot be
prevented by local hashes or advisory locks; same-user/reverted races and ACLs
remain limits. Current coding-UID daemon control defeats same-host root ownership
as an independent trust boundary. Human-reviewed protected ABI/artifact/image
migration and externally qualified service deployment remain required.
