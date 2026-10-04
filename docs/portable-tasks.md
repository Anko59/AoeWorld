# Portable task planning

`aoe-harness task-plan` gives Codex, pi.dev and DeepSeek Harness the same bounded
contract, canonical guides and mandatory gate recipes. It launches no provider,
runs no validation, and grants no tool permissions. Its output is always
`authoritative:false`. No Claude SDK, model, account or API key is required.

## Contract

An existing absolute regular JSON file, at most 32KiB, supplies exactly:

```json
{
  "version": 1,
  "id": "repair-example",
  "kind": "bug",
  "candidate": "<full lowercase commit OID>",
  "base": "<full lowercase commit OID>",
  "registry_hash": "<actual canonical registry fingerprint>",
  "role": "coordinator",
  "provider": "deep-seek-harness",
  "status": "planned",
  "objective": "Repair the identified regression",
  "acceptance": ["Mandatory checks pass on the exact candidate"],
  "todo": ["Write a failing-before fixture", "Implement and review the fix"],
  "artifacts": [],
  "rounds_remaining": 8
}
```

OID fields are full immutable SHA-1/SHA-256, not branches or abbreviated hashes.
Use the actual `registry_hash` returned by `ci-select`; do not invent a hash.
Unknown fields, arbitrary commands, publisher/merger roles, invalid status-role
pairs, namespace escapes and oversized inputs reject. IDs are bounded lowercase
letters/digits/hyphens. Acceptance/todo have 1–16 bounded items each. Budgets are
0–256 rounds; an exhausted contract must explicitly record `blocked`. This does
not modify an agent provider's persisted goal or grant automatic continuations.

Closed roles are coordinator, implementer, tester, reviewer, bounded maintainer
and QA. Closed task kinds are feature, bug, refactor, performance, geodata, QA,
read-only release-inspection and policy-upgrade. Semantic handoff suggestions
coordinate implementation→testing→review→QA→ready-for-human. A JSON role/status
is not an authenticated actor, test result, independent review or merge approval.

## Invoke

Create an external evidence directory, then run:

```sh
HARNESS_TASK_FILE=/absolute/external/task.json \
HARNESS_EVIDENCE_DIR=/absolute/external/task-evidence make task-plan
```

The Make facade builds in Docker, mounts the task file read-only and delegates to
Rust. The coordinator is not a restricted trusted supervisor: it retains normal
local development mounts/capabilities. Do not supply secrets or claim isolation.
Provider binaries are probed only by their fixed `codex`, `pi` or `dsh` name with
`--version`; the Docker coordinator may not contain binaries installed on the
host. Absence or invalid version output is `UNAVAILABLE` capability evidence,
not a reason to stop provider-neutral planning. Binary presence/version never
implies SDK pre-tool interception, filesystem enforcement or authenticated roles.

## Evidence and conservative selection

Both immutable candidate and base are raw, endpoint-checked snapshots. Registry
and role catalog are read from the candidate snapshot, not unstaged worktree
files. This is local feedback, not adoption of candidate policy by the protected
judge described in [judge preparation](judge-preparation.md).

The actual read-only Git comparison disables external diff/textconv/renames,
uses literal pathspecs and forced text with fixed Myers/no-indent settings. A
rename therefore supplies both deleted-old and added-new paths; task JSON cannot
supply affected paths or its own integrity verdict. Forced text prevents working
attributes from hiding textual assertion deletions. Non-UTF8/control-containing
binary changes may make planning unavailable; they are not silently dropped or
qualified by text heuristics.

Diff transport shares a 30-second budget, caps each command at five seconds and
requires complete bounded 64KiB receipts. The accepted complete diff additionally
has 4096-path/2MiB bounds; split oversized tasks rather than truncate evidence.
Existing raw snapshot/export/endpoint probes remain unsupervised and can have
larger memory costs. Endpoint checks cannot detect reverted same-user races.

Registry classification supplies the PR plan. Independently, the handoff always
contains the full `everything` preflight recipe. Role guidance cannot subtract
gates; an unavailable required capability remains incomplete. Context is a
16KiB serialized packet of exact references and guide-byte digests, not copied
guide content or permission-bearing prompt instructions. Canonical immutable
guide files are checked for UTF8, regular mode, bounded size and link aliases.

Referenced artifacts use `task-artifacts/<id>/...`; fixtures use the logical
`task-fixtures/<id>` namespace. Artifacts must already exist in the external
evidence directory, be bounded regular nonlinked files and match an explicit
full BLAKE3 digest. A digest binds observed bytes, not the author, exact-revision
semantic contents or review/test verdict. Namespaces are not filesystem access
controls or hidden-test confidentiality.

Atomic0600 outputs include the plan, complete accepted diff, bounded context and
version-probe receipt. A reused plan first becomes pending `UNAVAILABLE`; input,
comparison, digest, cancellation or other errors publish nonready evidence rather
than retaining stale ready output. Task and immutable source endpoints are checked
before publication. These same-user files are not authenticated supervisor logs.

## Integrity is independent review, never automatic PASS

Changed test ownership, deleted assertions/tests, ignore/only/should-panic,
proptest selection and campaign/coverage/threshold/budget/counter changes require
independent review. Protected or unknown policy scope also requires review.
Numeric decreases are additional heuristic findings. An empty finding set is
`REVIEW_NOT_ASSESSED`, never an integrity pass; every result retains independent
review requirements. Strings, macros, aliases, comments and multiline syntax can
fool these heuristics. AST-aware changed-test validation, canaries, mutation and
restricted hidden tests remain later work; this planner does not substitute for
them, browser coverage, real source/art evidence or hardware qualification.
