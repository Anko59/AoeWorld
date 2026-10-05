# Bounded QA evidence byte observations

The existing version-1 report wire format and `PASS`, `FINDINGS`, `BLOCKED`
claims remain in use. [The QA validator](../crates/harness/src/qa.rs) now prints
an actual byte observation, not an independent QA verdict. Build and scenario
strings remain claims; even a matching health/build SHA does not qualify the
served binary or runtime.

## Measured inputs

[The observer](../crates/harness/src/qa/observation/mod.rs) reads a held regular
report file, bounded to 4 MiB. [The shared JSON reader](../crates/harness/src/input_json/mod.rs)
rejects duplicate keys at every depth before constructing JSON objects, with
32-depth, 200,000-node and 16,384-entry collection limits. Exact-review parsing
uses the same unchanged limits. Reports and nested journeys/findings reject
unknown fields; printable text and finite list counts are checked. Budget names
`fast`, `full`, `extended` are closed schema values, not permission grants.

Artifact paths keep version-1 semantics: absolute paths or paths relative to the
current working directory, not implicitly joined to the evidence root. Every
artifact must have normal components and remain under the evidence root. The
report may be within that root but cannot be its own artifact. Symlink leaves
and ancestors, dangling links, hardlinks, directories, devices, FIFOs, missing
files, escapes and parent traversal are rejected. Unix identity checks fail
closed where unsupported.

[Held-file reads](../crates/harness/src/qa/observation/io.rs) use no-follow and
nonblocking opens, require regular single-link files, correlate path and FD
identity, and bound each read to the observed length plus one. Size, mode,
mtime/ctime and device/inode observations are checked before and after reads.
Evidence limits are 16 MiB per file, 64 MiB aggregate and 128 unique artifacts.
All retained files and the report are reread and raw-byte hashed at endpoints.
These observations do not close every reverted hostile race or impose hard
filesystem wall deadlines.

## Logical observation

The generated inventory sorts root-relative artifact names and purpose
references: journey name or finding index. Repeated references within one
purpose are rejected; a shared artifact across journeys is recorded once with
multiple purposes. Shared bytes do **not** prove that each journey executed.
Binary bytes are allowed; a hash does not interpret a PNG or validate a visual
claim. No raw image/report text or absolute artifact path is printed.

The report's actual raw BLAKE3 hash and length plus measured artifact paths,
lengths, raw hashes and purpose associations enter a typed compact JSON payload.
The BLAKE3 domain is `blake3:aoeworld-qa-evidence-v1` followed by NUL. The generated
manifest digest is outside that payload. This is not general RFC-canonical JSON.
Physical inode/time metadata and the input report filename do not enter the
logical payload. Changing absolute paths in the **raw report** changes its raw
hash and whole manifest even when relative artifact inventories remain equal;
whole-report relocation portability is not claimed.

Every result states `STRUCTURAL_EVIDENCE_OBSERVED_NON_AUTHORITATIVE`, with the
report's `claimed_status` separate, `authoritative: false`, independent QA and
journey execution `NOT_ASSESSED`, source identity and served build binding
`UNAVAILABLE`. No caller-supplied hash creates measurement or approval. Original
assets, geographic source and hardware qualification remain unavailable.

This slice does not independently authenticate QA, bind runtime artifacts,
qualify Docker worker ownership/isolation, supervise blocking browser transport,
or implement a full mutation campaign. Existing code tests use real bounded
files and path attacks; a compiled known-bad QA mutation canary remains separate
work. Coding-UID daemon control, hashes, signatures and root metadata do not
create independent judge authority. Raw reports/images may contain credentials;
no automatic public artifact upload or universal redaction is introduced.
