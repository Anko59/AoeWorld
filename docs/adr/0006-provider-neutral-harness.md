# Provider-neutral development harness

## Decision

Extend `aoe-harness` in Rust; do not maintain a separate quality policy in each
agent runtime. Git hooks, Make dispatch, CI, generated documentation, and runtime
adapters consume one executable registry. JSON remains the configuration format.
Keep `make preflight` as the minimum handoff gate and preserve existing coverage
floors, benchmark baselines, and the verified `dev` → artifact → `main` flow.

Codex, pi.dev, DeepSeek Harness and Claude Code adapters translate events and
findings only. The Claude Code adapter ([Claude Code](../claude-code.md)) is the
one that uses its runtime's pre-tool hooks: committed hooks judge each call.
Canonical instructions remain in AGENTS and provider-neutral task guides.
Runtime integration must report observed capabilities and versions. Unsupported
pre-tool interception or isolation is an explicit limit, never an enforcement
claim. A prompt-provided role is not authenticated identity.

## Threat model

Local guidance and ordinary Git hooks reduce mistakes; they are not a hostile
same-user process boundary. Candidate tests execute arbitrary repository code.
Mode 0600 and hashes cannot prevent that same user from forging writable evidence.
Strong role/evidence isolation requires separate restricted execution and a
launcher-controlled identity. Candidate execution must not receive merge,
publication, or approval credentials or unrestricted Docker-socket access.

Prefer structured operations and command allowlists over parsing arbitrary shell.
Do not promise that a runtime lacking pre-tool hooks enforces live role policy.
Malformed supported protocol input fails closed, with a remedy. Human cancellation
must remain possible; exhausted agent rounds produce a blocked task record.

## Trusted judging and evidence

A trusted run resolves a protected integration-policy commit once. Its trust
closure includes registry, runner, build/lock configuration, and actual tool-image
identities, not just the executable. Candidate policy changes receive self-tests
and human review; they cannot automatically replace their own judge. Bootstrap or
worktree judging is labelled non-authoritative. A protected policy upgrade is an
explicit transition, not a universal bypass flag.

Selection records the base and candidate. Execution records the actual snapshot:
working tree for feedback, index for commit, immutable commit for submission.
Unknown paths and unavailable comparison information select conservatively.
Renames include both names; instruction/harness protection precedes docs matching.
Selected but unavailable work is incomplete, not PASS.

Only evidence for the exact revision, tree, policy, tools, and required checks is
submission evidence. Reviews also bind the base and revision; rebasing invalidates
review readiness. Source identity is checked before and after long operations.
Endpoint checks cannot detect edits reverted between probes; authoritative builds
need immutable inputs or isolation, not an unsupported claim of atomicity.

## Roles and contracts

Coordinator, Tester, Implementer, Reviewer, and QA roles share portable task state.
Rust inline tests require syntax-aware changed-region policy or external test
modules; filename ownership alone does not separate duties. Hidden tests require
real isolation, not an agent-writable directory with a prohibition in prose.

Contracts cover independently versioned gameplay/diagnostic protocols,
deterministic replay, map identity/provenance, resource persistence/revisions,
worker cancellation/publication, asset formats, QA tools, and release manifests.
Token budgets apply only to model-facing text, not gameplay contracts.

Synthetic fixtures, native coverage, browser execution, real-art/source sessions,
and dedicated hardware are separate evidence categories. A missing qualification
cannot be filled by an unrelated passing workload. Keep existing richer performance
verdicts, including UNBASELINED and INCONCLUSIVE.

## Delivery and limits

1. Characterization, strict configuration, rename-safe paths, exact hooks, and
   retained-source release checks.
2. Executable registry, shared cadence runner, exact index scopes, and evidence.
3. Trusted execution, protected policy classes, and image identity.
4. Portable roles/tasks/adapters and test-first review.
5. Domain traceability, canaries, exact-review submission, QA and nightly triage.

This decision describes the target, not proof that all stages exist. Each pull
request must state implemented behavior, exact commands/revisions, failing gates,
CI evidence, and enforcement/qualification limits. Humans merge; agents never
approve their own changes, merge, or directly update `dev`/`main`.
