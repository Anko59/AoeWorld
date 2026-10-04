# Exact logical review subjects (local, non-authoritative)

`review-subject` materializes a closed logical subject from retained immutable
base and candidate snapshots. It does not run tests, authenticate a reviewer,
authorize submission or produce a protected judging verdict.

## Inputs and outputs

The CLI requires a full lowercase base commit ID and exactly one full candidate
commit ID or `--index`. Commit snapshots use independent resolution, ignoring a
caller's effective-index override. Index mode captures the intentional effective
index once; its pending tree is not represented as a candidate commit.

`--output` must be an absolute existing directory outside the checkout and common
Git metadata. Optional `--review` is an absolute regular, non-symlink/non-hardlinked
JSON file, at most4MiB. Input/output aliases, including unresolved symlinks, are
rejected before pending output replaces any previous observation.

A thin Make dispatcher supplies `REVIEW_BASE`, `REVIEW_CANDIDATE`, `REVIEW_OUTPUT`
and optional `REVIEW_INPUT`. Set `REVIEW_INDEX=1` instead of a candidate for index
mode. This Make wrapper rejects any defined `GIT_INDEX_FILE`, including an empty
value: forwarding an external index without a guaranteed explicit bind mount is
not supported, and it must not silently select another index. To use an intentional
external index, invoke the CLI in an environment where that exact index is explicitly
available/mounted. The CLI still captures intentional `GIT_INDEX_FILE` once. The
wrapper also rejects commas in output/input paths before Docker's CSV mount parser;
`--mount` requires existing host paths rather than creating missing directories.
The Rust CLI owns source/subject validation; Make arguments are not protected policy.
For direct CLI use, `--base <fullOID> --candidate <fullOID> --output <directory>`
or `--base <fullOID> --index --output <directory>` selects the subject.

- `subject.json` contains the logical subject. Its schema1 algorithm is
  `blake3:aoeworld-review-subject-v1`; compact typed serialization is hashed with
  domain `aoeworld:review-subject:v1` followed by a NUL byte. The digest is reported
  separately in the observation, avoiding self-reference. This is not RFC8785/JCS.
- `observation.json` starts unavailable and marks the final local result only
  after repeated retained-snapshot checks and unchanged review-input reads.
  On failure it remains unavailable; old subject bytes do not establish acceptance.

The subject includes both complete logical content witnesses, exact commit/index
identity, checked contract digest and actual AST bindings, separate canonical
registry fingerprint, complete affected paths, the candidate's PR plan and the
mandatory preflight-everything plan. Binding file BLAKE3 digests are matched to
actual witnessed files. Affected paths compare both inventories by path, mode,
blob ID and raw BLAKE3; rename/delete/add names are retained without hunk truncation.
The candidate registry is feedback, not protected minimum judge policy. Existing
required preflight and native85%/critical90% gates remain independent and unchanged.

Physical index paths/cache bytes, incidental current HEAD for commit mode and
private Git representation fingerprints belong only to the observation, not the
portable comparator. Logical tree closure does not certify ancestor-history
contents. Index source-HEAD commit bytes anchor source capture; they do not prove
private import of that commit or history. Repository anchor is `UNAVAILABLE`.

## Optional review document

The only top-level fields are `schema` (1), `subject` (the exact generated JSON
subject) and `notes` (nonempty printable text, at most4096 UTF-8 bytes). Duplicate
JSON object keys at every depth are rejected before ordinary Value deserialization.
Depth32, total200000 nodes and16384 members per array/object bound parsing. Unknown
nested fields, changed algorithms, reordered/duplicated arrays, altered bindings,
plans, paths or base/candidate identities cannot equal the generated subject.
Approval, budget, provider-role, command, credential and qualification fields are
not accepted. Notes are not echoed; their document byte hash observes bytes only.

Without a review the result is `SUBJECT_MATERIALIZED_NON_AUTHORITATIVE`. An exact
match yields `EXACT_SUBJECT_MATCH_REVIEW_NOT_AUTHENTICATED`. Both retain
`authoritative:false`, `independent_review:NOT_ASSESSED`,
`semantic_case_execution:NOT_ASSESSED` and `authenticated_verdict:UNAVAILABLE`.
Neither is readiness, independent review, approval or submission authorization.

## Limits

Local Git and filesystem operations are not hard wall-supervised. Endpoint checks
cannot detect every same-UID hostile edit reverted between observations. Atomic
mode0600/fsync/rename output is hygiene, not independently trusted directory
ownership or qualified power-loss durability. The coding UID's host-daemon access
still defeats local independent-judge claims. Protected built service artifacts,
restricted runtimes, authenticated reviewer/verdict evidence and deployment/operator
qualification remain separate unavailable evidence. Original game assets, geodata
and hardware qualification are not manufactured by these hashes or AST checks.
