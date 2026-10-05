# Local semantic calibration

The existing QA MCP catalog includes `run_semantic_case` with exactly:

```json
{"criterion":"movement-arrival-v1"}
```

This operation performs production `GameWorld` actions inside the controller.
It does not start or consult the browser worker, consume worker stdout, load
candidate reports, accept expected outcomes, or provide arbitrary execution.
There is no new CLI command, provider, image, or licensed-asset requirement.

## Performed predicate and controls

A fresh 256-by-256 world uses seed 7 and movement speed 128/1. One unit starts
at tile-center `(31,31)`, world position `(32256,32256)`. The controller captures
its initial state, performs an unknown-entity order for entity 999, and checks
that the typed rejection leaves unit, order, active movers and queries unchanged.
It issues a real order to `(33,31)`, position `(34304,32256)`, then captures each
of 16 actual advances. Fixed assertions check every tick's position, previous
position, movement/planning state, order destination/existence, active movers,
and old/target-tile membership. This crosses the 32-tile spatial-chunk boundary.
The first position is `(32384,32256)`; final arrival is exact and idle.

The normal case has 18 planned/performed actions and 178 fixed assertions.
Setup and spawning are prerequisites, not counted script actions. A second
fresh world deliberately omits the movement order but still performs all 16
advances: 17 script actions. The strong evaluator must accept the normal case
and reject the omitted-order case, including the final position predicate.
Both traces have the same deliberately fabricated six-completed-journey claims.
The actual legacy `qa::validate` structural checker accepts both claims. These
weak-control reports never enter the strong evaluator and their placeholder
evidence references are not actual retained artifact evidence.

`VALID_LOCAL_CONTROLS_OBSERVED` requires that measured 2-by-2 contrast and exact
counts. Missing guard rejection, unexpected weak rejection, unavailable setup,
or unexpected counts produce `INCONCLUSIVE`, not qualification. A missing
snapshot cannot become a zero or passing assertion. Observation failure after
some actions preserves the actual count rather than claiming no execution.

## Separate evidence and authority planes

Snapshots, assertions and outcomes are controller-constructed Serialize-only
Rust types; there is no deserialized verdict input. The requested criterion is
a fixed catalog enum. Raw MCP ingress uses the existing bounded duplicate-key
rejecting parser. Unknown arguments, alternate criteria, paths, approval roles,
completion flags and caller-selected actions are rejected before dispatch.

`LOCAL_CASE_PASS` is only this engine predicate, not six exploratory journeys.
The outer assessment is `LOCAL_SEMANTIC_CALIBRATION_OBSERVED_NON_AUTHORITATIVE`,
with `authoritative: false` and overall RTS `qualification: UNQUALIFIED`.
Independent QA remains `NOT_ASSESSED`; external judge capability, source identity
and served-build binding remain `UNAVAILABLE`. Root cause is always
`ROOT_CAUSE_NOT_ASSESSED`. In-process actions have no numeric child exit code:
`process_exit` is `NOT_APPLICABLE_IN_PROCESS`, never fabricated process success.

Retention is `CONTROLLER_MEMORY` during response construction only. Publication
is `MCP_RESPONSE_ONLY`, not a durable authenticated receipt or archive. Stdio
transport failure returns an error; successful MCP response generation does not
qualify the game. Neither legacy `record_journey`/`finish` nor source hash matching
can promote this local observation into independent authority. The controller's
compiled engine is not a measurement of unstaged working source or a served
candidate binary. Same-coding-UID binaries, daemons, Docker and hashes are not
independent judges. Protected external criteria/judging are needed for AUTH.

Original assets, rendered browser semantics, reconnect, audio, out-of-scenario
behavior, and dedicated hardware qualification remain outside this criterion.
Local calibration is useful functional execution, not whole RTS QA completion.
