# Checked contracts and native test traceability

[The contract catalog](../gates/contracts.json) records three deliberately narrow
requirements and nine existing native test bindings. It is provider-neutral data,
not a prompt role, permissions document, workload launcher or judging authority.
The canonical Rust checker validates structural relationships and source bindings;
it does not decide that the stated behavior is correct merely because a test exists.

```sh
make contracts-check
make architecture-check
```

The standalone command prints a structural summary and accepts no custom catalog,
source binding, output-file or qualification flags. The same Rust contract check is
part of `architecture-check`, rather than a duplicated YAML or Make policy.
Existing static CI and index-hook architecture checks therefore include it. This
adds no separate CI-selection gate and changes no native coverage floors or runner
budgets. Validation results for a particular revision must be reported separately;
this document does not claim that CI or any behavioral tests passed.

## Closed schema

The fixed catalog is bounded to128KiB of JSON with explicit `schema:1`. Unknown
fields are rejected at every level. Its only top-level collections are
`requirements`, `cases` and `deferred`:

- A requirement has `id`, `risk` (`critical`, `high`, `normal`), `scope`,
  `statement`, nonempty `invariants` and `gate_refs`.
- An invariant has a globally unique `id`, `claim`, nonempty `boundaries` and
  `case_ids`. A boundary has repository-relative `path`, file-local `symbol` and
  `kind` (`function` or `method`). Repeated boundary references must agree.
- A case has unique `id`, claimed purpose `kind` (`positive` or `negative`),
  `path`, file-local `symbol`, `runner:"native-test"` and `gate:"test-unit"`.
  Each path/symbol identifies one case, not multiple alternative meanings.
- A deferral has distinct `id`, `capability` (`source-assets` or `source-geodata`)
  and a nonempty `reason`. It cannot claim that external inputs are qualified.

IDs use lowercase ASCII letters, digits and hyphens, at most96characters. Text is
nonempty, bounded to1024UTF-8bytes and contains no control characters. Collection
bounds are enforced by the checker. Empty relationships, duplicate IDs, unknown
references, conflicting bindings and unused cases reject. Each requirement's
`gate_refs` is exactly the union of its referenced case gates; those gates must
exist in [the gate registry](../gates/registry.json). Critical and high-risk
requirements need both positive and negative case bindings.

There are no author, provider, command, CID, token, publisher, permission, enabled,
qualified, pass, budget or threshold fields. Attempting to supply one rejects
rather than selecting a default or silently granting a capability. Actual workload
budgets and gate execution remain owned by the validated registry and runner.

## Source binding, not filename matching

The checker parses actual Rust syntax and follows the supported crate/module chain
from a crate source root. A file containing a plausible function name, comment,
string or filename suffix is not sufficient. Repository-relative regular files,
module reachability, unambiguous symbols and the declared function/method kind must
agree. Production boundaries must not be test-only items. Method symbols include
the actual impl type, for example `GameWorld::issue_move_waypoints`.

Inline test symbols include their file-local module, such as
`tests::messages_round_trip`. External test files use their local function name,
not a fabricated crate-qualified namespace. Their actual parent module chain must
reach a test configuration. Bound cases require real native `#[test]` functions,
not merely test-shaped names; ignored, panic-only, unsupported generated or
ambiguous configuration bindings cannot silently count as valid native cases.
Symlinks, escaped or missing paths and unsupported module/configuration shapes
reject instead of falling back to another file.

The supported assertion inspection rejects obvious vacuity such as an empty test,
literal true or self-equality assertions. A recognized nonvacuous assertion is a
structural prerequisite, not proof of production behavior. Assertion discovery
has an explicit supported subset; uncertain macros or unsupported syntax must not
be invented into evidence. It cannot establish all semantic equivalences, detect
all mocked implementations or replace execution, mutation testing or independent
review. Test purposes in the catalog are claims about selected fixtures, not
certification of all positive or negative behaviors.

## Initial scope

| Requirement | Existing fixture scope | Excluded claims |
| --- | --- | --- |
| Diagnostic wire bounds | Client/server codec round trips, oversized frames and collections, malformed advertised vector length | Complete gameplay protocol, transport security or production load qualification |
| Fixed waypoint order | Synthetic predetermined route retains destination/queue; off-center and repeated inputs leave the initially absent order absent | Complete path planning, all impassable-edge cases, real geographic long routes or finished RTS performance |
| Effective index witness | Alternate staged source, invalid index locations, raw referenced blob substitution | Hostile same-user reverted-race protection, authenticated review or independent judge authority |

The waypoint rejection fixture's name mentions walkable edges, but its checked
inputs exercise off-center and repeated points; the catalog does not extrapolate
that name into comprehensive terrain-edge coverage. The index rejection fixture
checks a symlink path in its native Unix block; this is not a platform-independent
claim about every filesystem. Scope strings preserve these limits.

## Observations and future exact review

Successful structural checking reports `STRUCTURAL_REFERENCES_VALID` with
`authoritative:false`. Behavior/semantic test evidence and coverage remain
`NOT_ASSESSED`; missing original assets or real-source geodata remain `UNAVAILABLE`.
A deferral never counts as a checked behavioral case. The native85% overall and90%
critical coverage gates stay separate and unchanged; line coverage alone is not
semantic proof. Browser, source-backed and dedicated hardware qualification remain
separate work described in [testing](testing.md).

A future exact-review subject must bind an independently computed contract digest
in addition to the canonical registry fingerprint. Changing this catalog does not
by itself change the registry fingerprint. Index/commit source, tree and raw-object
witnesses, affected paths, required gate plans and actual execution receipts must
also be bound at materialization time, not supplied as self-referential revision
fields in this catalog. That exact-review adapter is planned, not implemented by
this catalog or the structural checker.

Current coding-UID host-daemon control means root-owned files, local hashes,
signatures or prompt roles cannot create an independent trusted judge on this host.
No observation here authorizes workers, verdicts, publication, approval or merging.
Protected artifact/runtime measurement and externally qualified service deployment
remain separate requirements; human source merges alone do not qualify them.
Original game assets stay outside Git and images. See [supervisor IO](supervisor-io.md)
for the existing model-only observation and filesystem limits.
