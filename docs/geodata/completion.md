# Geographic map completion ledger

Implementation starts at `04464752609a4bcd74a7d4d4c5943f20233dac65`.
Preserve existing work; complete the following acceptance-driven milestones.
Use isolated worktrees for independent edits and an independent review before
integration. All compilation and validation uses Dockerized Make targets.

## Product contract

Select a WGS84 geographic square and a compression ratio for circa 600 CE.
Geographic fidelity wins over competitive balance. Tiles are two game meters;
cavalry moves at three game meters/second and reference walking speed is 7/6
meters/second. Elevation uses one-game-meter steps and shared ramps/cliffs.
Support 250 m through 10,000 km selections, ratios 1 through 10,000, and 64
through 262,144 tiles per side without dense fine-tile allocation.
Keep modern evidence, historical models, procedural detail, and fallback data
distinguishable. No economy, buildings, combat, or naval units in this scope.

## Milestones

| ID | Deliverable | Status | Acceptance |
| --- | --- | --- | --- |
| M1 | Correctness repairs | Implemented; revision checks pending | Asymmetric routes; no endless budget retry; valid HYDE missing masks; correct extent/edge chunks; bounded starts; native tests pass |
| M2 | Detailed streaming preparation | Pending | Regional Copernicus inputs; directory package; bounded page preparation/residency; offline unseen chunks; integrity/cancellation |
| M3 | Water and historical reconstruction | Pending | WorldCover/HydroLAKES/HydroRIVERS; coherent water/barriers; whole-cell HYDE allocation; correction format; representative regions |
| M4 | Physical movement and long routes | Pending | Fractional/waypoint distance carry; resumable fair search; lazy connectivity and detours; slope consistency; 100 km travel/replay |
| M5 | Resource lifecycle | Pending | Independent ore streams; obstruction-aware access; bounded resource deltas; persisted overlays; eviction/reconnect/reload |
| M6 | Terrain and resource rendering | Pending | Reviewed missing art; ramp/cliff/water meshes; transitions; surface picking/occlusion; covering LOD; bounded requests; both backends |
| M7 | Creation experience | Pending | Accurate estimates/detail; useful preview; create/cancel/retry/open; atomic activation; bounded job retention/recovery |
| M8 | Qualification | Pending | Source-backed workloads at 512, 16384, 50000 tiles and sparse maximum; parser fuzz/coverage/CI; final revision evidence |

## Integration rules

- Never turn a failing gate into a pass by weakening limits or assertions.
- Write direct adversarial regression fixtures, not searches for convenient
  generated examples. Preserve determinism across query order and cache state.
- Keep original/derived game art and real-data outputs outside Git and images.
- Before committing run hooks-install, hooks-check, focused gates, preflight.
- Record exact revisions, commands, results, limits, and PR URLs below.
- A milestone is complete only when its user-visible behavior and acceptance
  tests pass. Small accessor/telemetry changes do not close a milestone.

## Baseline evidence

At the starting revision, `make map-test` passed 39 tests. `make test-unit`
passed 223 of 224 tests; the no-land-start activation integration test failed
with an HTTP read timeout. Existing browser/WASM/performance reports predate
that revision. The current preparation path is fixed at 128 samples per axis;
approximately 6 GiB of geographic inputs are cached but detailed datasets are
not yet wired into normal generation.

## Handoffs

M1 changes have been independently reviewed and integrated. The inherited
foundation is preserved in draft [PR #8](https://github.com/Anko59/AoeWorld/pull/8)
at the starting revision. Integration and acceptance remain the orchestrator's
responsibility.

Pre-commit checks on the combined M1 tree: `make preflight` passed all static
gates, 232 native tests, doctests, WASM build, and the synthetic smoke; `make test-e2e` passed 21 tests;
`make test-wasm` passed seven actual browser cases (2 integration, 3 client,
2 rendering). The expanded browser gate now executes the new map regressions.
`make map-generate` and `make map-verify` succeeded for the 30 km Paris overview
using seven cached inputs (6,043,158,637 bytes; zero downloaded bytes). Package
hash: `8f16ee0f746ceead92a918ae08b442c01cc6105f77eb7d75edac3c58aa35ecf2`.
This is source-backed overview evidence, not detailed-data or performance
qualification. Final revision-bound checks and the repair PR are pending.

M2 directory package implementation is delegated separately. Preparation and
runtime residency remain dense until their acceptance tests prove otherwise.
