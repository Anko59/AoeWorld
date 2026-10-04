# Artifact signature observations

`supervisor-model` can inspect a signature over its strictly parsed artifact
subject. This is actual strict Ed25519 verification, **not** a trusted service
admission, executable/runtime measurement, protected build provenance or validation
verdict. Every observation retains `authoritative:false` and
`admission_granted:false`. The containing report still says `UNAVAILABLE` even
when the algorithm matches an observed root-key file.

## Signed bytes and envelope

The optional top-level `attestation` field is strict schema1 JSON with only
`schema`, `key_id` and `signature`. No caller public key, algorithm, verifier
command, key path, trust boolean or unsigned-success fallback is accepted.
Signature is exactly128 lowercase hex characters (64bytes). Key ID is64 lowercase
hex characters and must equal the subject's `trust_root_id` and observed key ID.

The message is the following concatenation, without a trailing newline:

```text
UTF8 bytes of "aoeworld:protected-artifact-subject:v1" followed by NUL
serde_json serialization of the validated typed subject
```

Typed serialization preserves the subject struct's field order and the ABI array
order; signatures bind those exact bytes, not a caller's whitespace variant.
Subject serialization order is schema, repository_id, protected_ref,
protected_commit, protected_tree, closure_blake3, registry_hash,
executable_blake3, runtime_blake3, trust_root_id, abi. The nested ABI uses its
existing typed serialization. Strings use serde_json escaping. There is no generic
JSON canonicalization claim: signers must produce the same typed serialization
and versioned domain. The signature observer accepts at most16KiB subject bytes.
Changing repository/ref/commit/tree, whole closure, registry, executable/runtime,
root identity or image manifest/config/dispatch changes the signed message.

Key ID is BLAKE3 of:

```text
UTF8 bytes of "aoeworld:artifact-key:v1" followed by NUL
32 raw Ed25519 public-key bytes
```

The verifier uses pinned `ed25519-dalek`2.2.0 with `verify_strict`, rejects weak
public keys and does not provide signing functionality in the production command.
Test-only deterministic private keys qualify algorithm/binding regressions, not
an actual operator, service, image or build.

## Fixed key-file observation

Production consults only `/etc/aoeworld/supervisor/trust-key.json`. There is no
CLI/environment key-path override or candidate key fallback. Strict bounded4096byte
JSON fields are `schema`1, positive numeric `repository_id`, `public_key`64 lowercase
hex, and `key_id` as above. The key's repository ID must match the subject's anchor.

Unix ancestors must be nonlinked root-owned directories without group/world write.
The leaf must be root-owned, single-linked, regular and exactly0600, without special
mode bits. A no-follow/nonblocking opened handle must match initial metadata;
bounded reads and endpoint handle/path/ancestor metadata and same-handle bytes are
rechecked. Missing, unreadable, malformed, linked, oversized or changed files never
select another key. Non-Unix ownership support is unavailable. The Make model
container does not mount an operator root key; lack of access remains unavailable.

Observation statuses are `ABSENT` (does not consult a key), `REJECTED`,
`UNAVAILABLE_KEY` and `VERIFIED_SIGNATURE_NON_AUTHORITATIVE`. Successful matching
reports subject/key-file BLAKE3 observations, not an authenticated approval token.
All reasons are bounded generic messages, not supplied signature/key dumps.

## Authority boundary

Root ownership and a matching signature cannot prove that a root key belongs to an
approved independent operator. In particular, the coding UID's Docker-daemon
control on this host can rewrite root-owned host state. Same-user and reverted
races, ACLs, root inside a container namespace, service IPC/credentials and alternate
daemon endpoints remain outside this observation. No actual executable, runtime,
source export, OCI config or container is read or admitted by the signature check.
A signer can sign inaccurate assertions; all those facts still require independent
measurement by a qualified protected service.

Human-reviewed protected ABI/real image/artifact migration and an external trust
boundary remain required. No keys, root services, permissions or daemon controls
are installed or changed by this command. Missing qualification stays unavailable;
no signature result enables workloads, hidden-test access, role permissions,
approval, publication, merge, release promotion or hardware qualification.
