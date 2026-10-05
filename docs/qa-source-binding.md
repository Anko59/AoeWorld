# QA evidence bound to an actual immutable logical source subject

The optional `qa-validate` source adapter matches version-2 expected hashes to an
actual generated source summary. It is **not** runtime build qualification,
independent QA execution, authenticated review, or submission authorization.

## Closed report format

```json
{
  "version": 2,
  "report": {
    "version": 1,
    "budget": "fast",
    "build": "unverified-health-claim",
    "scenario": "exploratory-session",
    "status": "BLOCKED",
    "journeys": [],
    "findings": []
  },
  "expected_source": {
    "candidate_content_witness_digest": "<full lowercase 64-character BLAKE3 hex>",
    "review_subject_digest": "<full lowercase 64-character BLAKE3 hex>"
  }
}
```

The example placeholders must be replaced with hashes from the actual generated
logical subject. All nested structures are closed; duplicate JSON keys at any
depth fail before object construction. Expected hashes are compared, never
accepted as source measurements. The existing bounded report/artifact reader
hashes the **entire original wrapper bytes**, not a normalized inner report.
Its limits, path guards, purposes and retained-file endpoint checks are unchanged.

## CLI and immutable scope

```text
aoe-harness qa-validate report.json --base FULL_BASE_OID --candidate FULL_CANDIDATE_OID
aoe-harness qa-validate report.json --base FULL_BASE_OID --index
```

The repository is the current working directory; evidence remains under
`reports/qa`. No arbitrary repository, output or provider-command option is added.
Base and exactly one candidate commit or intentional index must be supplied as a
group. Full lowercase Git object IDs are required. Commit snapshots ignore the
ambient index; an intentional index is captured once. Its pending tree and
captured source HEAD are not an invented candidate commit.

The legacy optional positional argument remains supported, including the default
`reports/qa/session.json`. Version 1 without source arguments retains the existing
byte-only non-authoritative observation. Version 2 without immutable context fails;
version 1 with immutable source arguments is explicitly unsupported. Existing MCP
finish still uses the version-1 byte adapter, not this source-matching route. The
unchanged Make target does not forward new immutable CLI arguments.

## Measured matching and held inputs

The adapter retains both actual snapshots and uses the same exact-review
materialization/digest generator, including checked AST/catalog and registry,
affected paths, base and complete mandatory plans. Its summary contains actual
candidate/base logical content digests, exact review subject digest and scope.
No caller-supplied JSON summary or physical snapshot fingerprint replaces it.

A summary is generated before evidence measurement. Original report and artifact
FDs remain held through comparison and another generation of that same summary;
all original bytes and path/FD identities are reread at late endpoints. Final
logical witnesses also recheck raw source objects, private snapshot state and
exports. Replacing files cannot be legitimized by opening fresh handles. Endpoint
observations do not close every hostile reverted race or impose filesystem wall
bounds.

Output is stdout only; failures produce no accepted observation file. The composed
result states `SOURCE_AND_EVIDENCE_MATCH_OBSERVED_NON_AUTHORITATIVE`, with an actual
`EXACT_GENERATED_SOURCE_MATCH_NON_AUTHORITATIVE` summary and `authoritative: false`.
Top-level source identity is `MEASURED_LOGICAL_CONTENT_NON_AUTHORITATIVE`; the inner
byte-only observation retains its own `source_identity: UNAVAILABLE` to distinguish
its narrower assessment. Served build binding stays `UNAVAILABLE`, independent QA
and journey execution `NOT_ASSESSED`, and report status remains a claim.

Even a health/build string equal to a Git OID does not certify the served binary.
Hash agreement does not establish reviewer identity, protected judge authority,
hidden verdicts, worker ownership/isolation or approval. Coding-UID daemon control,
local signatures and root metadata cannot create independent authority. Licensed
assets, geographic source and hardware qualification remain separate and
unavailable. No raw reports, image contents or credentials are automatically
uploaded.
