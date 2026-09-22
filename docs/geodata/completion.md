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
| M1 | Correctness repairs | Verified; PR #9 | Asymmetric routes; no endless budget retry; valid HYDE missing masks; correct extent/edge chunks; bounded starts; native tests pass |
| M2 | Detailed streaming preparation | Regional prep/residency/creator verified; large extents pending | Regional Copernicus inputs; directory package; bounded page preparation/residency; offline unseen chunks; integrity/cancellation |
| M3 | Water and historical reconstruction | Units/dry cells repaired; reconstruction pending | WorldCover/HydroLAKES/HydroRIVERS; coherent water/barriers; whole-cell HYDE allocation; correction format; representative regions |
| M4 | Physical movement and long routes | Physical speed verified; routes pending | Fractional/waypoint distance carry; resumable fair search; lazy connectivity and detours; slope consistency; 100 km travel/replay |
| M5 | Resource lifecycle | Persistence and synchronization verified; source-session qualification pending | Independent ore streams; obstruction-aware access; bounded resource deltas; persisted overlays; eviction/reconnect/reload |
| M6 | Terrain and resource rendering | Art/stale-frame fixes verified; geometry pending | Reviewed missing art; ramp/cliff/water meshes; transitions; surface picking/occlusion; covering LOD; bounded requests; both backends |
| M7 | Creation experience | Creator and publication verified; recovery/progress pending | Accurate estimates/detail; useful preview; create/cancel/retry/open; atomic activation; bounded job retention/recovery |
| M8 | Qualification | Pending | Source-backed workloads at 512, 16384, 50000 tiles and sparse maximum; parser fuzz/coverage/CI; final revision evidence |

M7 completion publication is reviewed in
[PR #24](https://github.com/Anko59/AoeWorld/pull/24), revision
`12fac5bd1c117d49ed4ea3b96a00be61bbe35fc1`. Map registration precedes observable
job completion; cancellation during a registry wait prevents publication.
Both direct race regressions failed before the fix and pass afterward. Hooks,
test-unit (297), preflight, and clean pre-push preflight passed. One earlier
preflight failed the existing five-message WebSocket scenario-reset assertion;
unchanged retry and pre-push passed. No assertions or gate limits were changed.
This does not close disk recovery, cancelled staging cleanup, or job progress.

M4 resumable route planning is reviewed in
[PR #23](https://github.com/Anko59/AoeWorld/pull/23), revision
`dc3cb12c4e6095bd904197c6ce283472feff1f14`. Frontier state survives tick budgets;
round-robin scheduling and a 64-planner limit bound concurrent work. Provider
errors remain typed, exhausted local searches terminate, and retained search
entries are capped before insertion. Planner progress and budget participate
in replay hashes. Hooks, map-test (58), preflight (288 native tests), and clean
pre-push preflight passed. Cache byte accounting is a logical payload budget,
not allocator-inclusive RSS. Four-portal segment selection still needs the
long-detour connectivity work and source-backed 100 km qualification.

M8 native geodata fixtures are reviewed in
[PR #22](https://github.com/Anko59/AoeWorld/pull/22), revision
`d849e94ff5fd2d172ced61c52f88f3dbee4039e2`. Local raster/vector/ZIP and bounded
loopback HTTP fixtures exercise sampling, pyramids, HYDE extraction, and cache
resume/error paths. An ignored HTTP range now removes the stale partial file
so acquisition can restart. Hooks, test-unit (305), preflight, and clean
pre-push preflight passed. These fixtures do not establish the overall 85%
coverage gate or qualify real-world generation performance.

M6 camera elevation and sprite anchoring are reviewed in
[PR #21](https://github.com/Anko59/AoeWorld/pull/21), revision
`6eef8330fd179486d1345cff5e7d9b40140802bb`. The initial map focus uses loaded
terrain altitude, projection and picking share that focus, and Canvas sprite
anchors agree with WebGPU. Hooks, preflight (281 native tests), test-e2e (23),
and test-wasm (13) passed; clean-revision pre-push preflight passed. The rebuilt
browser bundles were compared on the private detailed Paris package in both
backends. The fixed eight-level culling margin and initial-only altitude focus
do not qualify high-relief traversal. Surface meshes, water geometry, and
occlusion remain open.

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
qualification. Revision-bound checks passed at
`76d15c04b1a5b17a47c7464cedacb7e54d261abf`: hooks-install, hooks-check,
map-test (42), preflight (232 native tests plus other gates), test-e2e (21),
and test-wasm (7). [PR #9](https://github.com/Anko59/AoeWorld/pull/9) is ready
for review. These are local Dockerized checks; no GitHub CI result is claimed.

M2 directory packages now use bounded manifests and individual coordinate
pages. Native preparation returns only the manifest; server startup verifies
pages in canonical order without retaining page payloads. Repeated source
generation and verification reproduced the Paris hash above, with seven cached
inputs and zero downloads. Preparation and runtime residency remain dense;
this storage slice does not close M2.
It is reviewed in [PR #11](https://github.com/Anko59/AoeWorld/pull/11), revision
`bec945b92d8f2a58d617da99325a11684cd5cef8`. Dockerized hooks, map-test (43),
preflight (250 native tests), test-e2e (21), and test-wasm (7) passed. Browser
checks, map-verify, and pre-push preflight passed on the clean revision.

M4 physical movement is reviewed in
[PR #10](https://github.com/Anko59/AoeWorld/pull/10), revision
`330f4ffc32ae0549f5cad5b9575057e0db26de7f`. Cavalry uses 3 m/s rational speed,
fractional carry survives ticks and redirects, waypoint remainders are consumed
within the tick, and exact off-center destinations are reached. Legacy speed
configuration remains readable. Dockerized hooks, preflight (242 native tests),
test-e2e (21), and test-wasm (7) passed; browser checks and pre-push preflight
ran on the clean committed revision. Resumable routing and source-backed
100 km travel qualification remain open.

M7 job history is reviewed in
[PR #12](https://github.com/Anko59/AoeWorld/pull/12), revision
`6c53a9e1e24da8141f6931ca426371cc41abde02`. It retains 128 records, evicts
only terminal work, rejects exhausted identifiers without mutation, and drops
the invented start-time ETA. Hooks, focused native tests (253), and preflight
passed; pre-push preflight passed on the clean revision. Saved-package quotas,
measured progress, resumable jobs, and crash recovery remain open.

M3/M5 generation correctness is reviewed in
[PR #13](https://github.com/Anko59/AoeWorld/pull/13), revision
`417e6ef5c8899a80bb741973a7488ebf3e23dd21`. Mountain material thresholds use
centimeters correctly, prepared dry cells stay dry, ore streams are separate,
and patch access accounts for deterministic object blockers. Generation recipe
2 changes content identity without reseeding geography; old maps-v5 packages
remain intact and compatible packages are generated under maps-v6. Hooks,
map-test (49), lint, preflight (239 native tests), test-e2e (21), and test-wasm
(7) passed. Pre-push preflight and browser gates passed on the clean revision.
Combined directory/generation regeneration verified the Paris overview under
maps-v6 as `7d8116e53ee7aba67f25cd63daa0259f4a9c55ecf734134641b929c1b75cc9ec`,
using the same seven cached inputs and zero downloads.

M6 resource art and viewport stability are reviewed in
[PR #14](https://github.com/Anko59/AoeWorld/pull/14), revision
`c26db8a644e7c0495303dcfb24bbec525f7f77b6`. Reviewed source contact sheets
identify forage, wood, gold, and stone roles; private assets remain outside Git.
Superseded subscription replies no longer trigger false protocol errors. Hooks,
preflight (233 native tests), asset verification (20,396 frames, 27 pages),
test-e2e (23), and test-wasm (7) passed. Pre-push preflight and browser gates
passed on the clean revision. Source contact-sheet review does not establish
in-game alignment: terrain coverage, ramp/cliff/water geometry, picking, and
actual geographic scene qualification remain open.

M2 regional preparation is reviewed in
[PR #15](https://github.com/Anko59/AoeWorld/pull/15), revision
`22f40f7b1b9b6ee8e4555ac5070fd341fb543b67`. Public Copernicus GLO30/GLO90
inputs feed disk-staged elevation pages and pyramids; coarse overview water,
vegetation, and HYDE retain their source locks and provenance. Hooks, focused
checks, preflight (259 native tests), and clean-revision pre-push preflight
passed. The 30 km Paris request at 1024 samples (about 29.3 m spacing) generated
and verified `701fc72311baa491113a4f3282d8f05c44c58ffe97a072b46cac0dc43219802e`
on that branch, using nine cached GLO30 tiles (406,204,594 source bytes) and
zero new downloads. This hash uses the branch's generation recipe; integration
with recipe 2 produces a separate content identity.
The combined tree also generated and verified the 1024-sample Paris package
under recipe 2 as
`ed81472c347ba4038733aba08901a817cc936e21c561dbf098e0f59a5c5b12f2`.

This explicit CLI/worker slice accepts at most 4096 samples, 64 regional tiles,
4 GiB of regional input, and 2 GiB of staging, with a separate 6 GiB overview
transfer preflight. Polar and antimeridian footprints are rejected. A missing
DEM cell becomes zero only when source-backed overview evidence is entirely
ocean; missing land or partial-coast data fails. Automatic creator integration,
larger/wrapped selections, detailed hydrology/history, and recovery remain open.

Combined revision `5a1a5338acd33273323b44ca8befc68e0d292c35` passed local
preflight (271 native tests), test-e2e (23), and test-wasm (7). Its
[manual CI run](https://github.com/Anko59/AoeWorld/actions/runs/35659384603)
passed static, browser, and fuzz checks but failed the performance bundle-size
comparison (258,301 bytes versus 217,240, with a 5% limit) and overall coverage
(11,030 / 14,416 lines, 76.5%, versus 85%). Native instruction/allocation and
critical coverage groups passed. Coverage inventory errors and real untested
paths are both being addressed; no overall CI pass is claimed.

The bundle-size failure is repaired in
[PR #16](https://github.com/Anko59/AoeWorld/pull/16), revision
`1adc0eb0f6462da74b2fbb294733b16e57ccf0d2`. A dedicated WASM profile is used
by Make and release artifact builds. The optimized gzip bundle is 222,723 bytes
against the unchanged 217,240-byte baseline and 5% allowance. Hooks, preflight
(271 native tests), test-e2e (23), test-wasm (7), and perf-ci passed; browser,
performance, and pre-push checks passed on the clean revision. Native profiles
and performance baselines were not changed. Full dev release packaging and
published release validation remain separate; native coverage gaps remain open.

M6 flat terrain coverage is reviewed in
[PR #17](https://github.com/Anko59/AoeWorld/pull/17), revision
`7116082b65e7aa3c6dd05fae33338a4c596711a3`. Native terrain diamonds are
normalized to the isometric tile footprint and centered anchor; bounded LOD
covers translated and zoomed viewports. Unit and resource scale is preserved.
Hooks, preflight (271 native tests), test-e2e (23), test-wasm (10), and
clean-revision pre-push preflight passed. Combined with detailed preparation
and the WASM profile fix, preflight (280 native tests), test-e2e (23),
test-wasm (10), and perf-ci passed. This is flat fixture coverage evidence;
altitude-aware cameras, surface meshes, picking, and actual source scene
alignment remain open.

M2 bounded runtime residency is reviewed in
[PR #18](https://github.com/Anko59/AoeWorld/pull/18), revision
`fcd487b4c24e09f75de05a8792bbe434dc87663d`. Verified pages load through a
128-page cache, with at most two indexed providers in the server registry and
a 64 MiB index allowance per package. Active handles can outlive registry
eviction. Chunk/preview/activation requests propagate page failures and use
request-local cancellation. Fixtures cover all-field dense/provider parity,
post-start corruption, legacy layouts, and eviction across a 341-page package.
Hooks, map-test (50), preflight (276 native tests), test-e2e (23), test-wasm (7),
and clean-revision pre-push preflight passed. The combined tree passed
preflight (285 native tests), test-e2e (23), test-wasm (10), perf-ci, and
verification of the detailed Paris recipe-2 package recorded above.
Maximum-pyramid qualification, automatic detail selection, wider geographic
support, and source-backed long travel remain open. Legacy convenience route
APIs fail closed but do not yet preserve typed provider failures.

M8 coverage inventory correctness is reviewed in
[PR #19](https://github.com/Anko59/AoeWorld/pull/19), revision
`5913268cede55fa0b4b40695764aefec4af8b627`. The native checker follows Cargo
production targets and Rust module paths, counts executable production spans,
and excludes test-only modules and non-executable files. Missing production
modules or coverage records remain failures. Hooks, native tests (280),
preflight, and clean-revision pre-push preflight passed; independent review
corrected module resolution and target-discovery omissions before publication.
The saved CI data now yields 11,222 / 14,658 covered lines (76.6%), below the
unchanged 85% floor, and lacks the new inventory module. The earlier provisional
73.92% result omitted production modules and is invalid. A fresh instrumented
run and additional production-path tests remain required; this repair does
not establish a coverage or overall CI pass.

The fresh instrumented run at
`ef0b0a8592fa59f742fac38753f8067f75dfed18` measured 12,456 / 16,960 lines
(73.4%), with no missing sources and all critical groups passing. It still
fails the 85% overall floor. The larger denominator includes the subsequent
detailed-preparation and residency code.

[PR #20](https://github.com/Anko59/AoeWorld/pull/20), revision
`171ebf638f5db9716221ecb025bd17b896d3481a`, fixes a separate instrumentation
gap: browser coverage selects the instrumented native server and requires
graceful shutdown to flush counters. Independent review, hooks, preflight
(295 native tests), browser-check, normal E2E (23), instrumented E2E (23),
WASM (10), and clean-revision pre-push preflight passed. Server and harness
profiles were produced. The full coverage command remains a failure at
12,660 / 17,000 lines (74.5%); missing sources are empty and critical groups
pass. No threshold changed. Failed browser runs may force cleanup without
flushing server counters and cannot produce a passing report.
The current source generation recipe is version 3. It corrects source-grade
surface classification before quantization and therefore changes immutable
content identity while leaving the geography hash domain stable. Existing
maps-v5 and maps-v6 directories remain untouched; the default output directory
is maps-v7 and old packages must be regenerated from cached source inputs.
Resource placement retains its explicit recipe-2 detail domain so this terrain
recipe migration does not reseed published resource placement.

Source-slope classification is reviewed in [PR #25](https://github.com/Anko59/AoeWorld/pull/25),
revision `a41a022827099cc3a1c96d5f62dea6a9ee71f2dd`. Hooks, map-test (52),
preflight (296 native tests), test-e2e (23), test-wasm (10), and clean pre-push
preflight passed. Source grades above 35% remain impassable even when corner
heights quantize to one level. Resource detail keys keep their previous bytes.

The fresh combined coverage run at
`bdd2f66a923138287864b67acee8272c8ae625c5` measured 14,142 / 17,709 lines
(79.9%). No sources are missing, and critical groups pass; the unchanged 85%
overall gate still fails. Acquisition and worker orchestration need more tests.

The recipe-3 detailed Paris package generated and verified at integration
`629e9762b0348d9bb0d5391b1c119b79eea7486f` is
`3687f33815db6431fa007eb3f61d98e496dcfa92ac95bcaa2a0fb925069fafd9`, with
1024 samples per axis and 16 source locks under maps-v7. The previous maps-v6
artifacts remain intact. Combined preflight passed 323 native tests.

M2/M7 creator detail selection is reviewed in
[PR #26](https://github.com/Anko59/AoeWorld/pull/26), revision
`c4020f9482b6789e74d25dec20a3bea71f8089ec`. Automatic regional elevation uses
128–4096 samples targeting 30 m within the documented conservative window;
overview and detailed choices are explicit. Jobs and estimates disclose the
selected grid and source limitations. Missing workers and unsupported forced
detail fail explicitly; response validation rejects a changed sample axis.
Hooks, preflight (302 native tests), browser-check, test-e2e (23), and clean
pre-push preflight passed. Normal development worker startup and writable output
mounts still need their follow-up, then actual creator source-backed evidence.
Water/history detail, broader footprints, progress, and recovery remain open.

M7 normal development worker startup is reviewed in
[PR #27](https://github.com/Anko59/AoeWorld/pull/27), revision
`b161174b8f4b44180f10fc3aac443283a1754264`. Development builds the native
worker and mounts only package output and the canonical geodata cache writable.
The checkout remains read-only; missing or non-executable workers fail startup.
Hooks, preflight (324 native tests), E2E (23), and clean pre-push preflight
passed on that branch. Integration preserves the recipe-3 maps-v7 directory.
Actual source-backed creator verification remains pending.

The normal creator flow was verified at integration
`efc0755481fdf788a618123c40f4a4195d9b7c7d`: `make dev` built and ran the worker;
the browser estimated and generated default Paris at 1024 samples per axis,
then activated package
`c47b00d6915c97deeee26ae777876f916e6db31a0d31b9b92119d2cbe0967a48`.
It reports 16 source locks and 500 tiles per side. Reconnect loaded terrain
and the primary unit. Elevation transition gaps remain an M6 rendering issue.

M5 bounded snapshot foundations are reviewed in
[PR #28](https://github.com/Anko59/AoeWorld/pull/28), revision
`adf1b4d450916df1411c11e2620e8ba5776ca159`. Schema/map identity, canonical
resource IDs, amounts and revision history are validated before replacement.
Decoding and mutation cap changed resources at 65,536 logical entries; provider,
capacity and revision errors leave state intact. Independent review, hooks,
map-test, preflight (333 native tests), and clean pre-push preflight passed.
Disk persistence and reconnect/delta consumers remain required.

M5 server persistence is reviewed in
[PR #29](https://github.com/Anko59/AoeWorld/pull/29), revision
`7b0e9944defd81a480d669e0570e328d60777935`. Activation restores bounded resource
snapshots before start selection and spawning. Persistence validates a candidate,
atomically replaces the file, then commits live collision changes. Failed writes
leave the world intact; stale service revisions cannot overwrite newer saves.
One fixed staging file per map bounds crash leftovers and is recovered on retry.
One server process owns the directory. Directory-sync uncertainty after replacement
is explicit. Independent review, hooks, test-unit, preflight (339 native tests),
E2E (23), and clean pre-push preflight passed. This introduces no economy command;
network deltas and client sprite invalidation remain open. Combined integration
preflight passed 342 native tests.

M5 resource synchronization is reviewed in
[PR #30](https://github.com/Anko59/AoeWorld/pull/30), revision
`356a9a0259a0f29e37f937262f294dbdd9f0dfa4`. Protocol 7 sends bounded sparse
resets and exact-revision deltas, using a 1,024-entry server journal and full
reset when history is unavailable. The client retains at most 65,536 amounts
outside immutable chunk residency and filters exhausted sprites on both
backends. Malformed/discontinuous updates close the stream and clear state;
empty advancing deltas and increasing known amounts are rejected atomically.
Independent review, hooks, preflight (346 native), WASM (13), E2E (23), perf-ci,
size, and clean pre-push checks passed on the feature branch. Combined
integration passed 349 native tests, E2E (23), and perf-ci before the final
malformed-stream closure follow-up. Source-backed interactive depletion and
long-session qualification remain open; no economy command is introduced.

The final wire integration preflight rerun hit the previously observed diagnostic
WebSocket scenario-reset assertion (`malformed_handshakes_resync_and_scenario_change_are_visible`):
348/349 passed. An unchanged retry passed 349/349 and the size check. The
five-message test assumes a small pending tick backlog; no assertion or gate
was changed. This intermittent qualification issue remains recorded for M8.

M7 worker lifetime is reviewed in
[PR #31](https://github.com/Anko59/AoeWorld/pull/31), revision
`3e972f7959096b7b4ec5decf1dca228a44cdcd95`. The Dockerized Linux server owns a
worker process group, drains bounded output, writes requests independently,
and terminates worker/GDAL descendants before joining pipes. Cancellation,
overflow, normal exit, and deadlines have process-level regression fixtures.
Preparation is limited to two hours, projection to 30 seconds. Independent
review, hooks, preflight (346 native), and clean pre-push checks passed.
Combined integration preflight passed 353 native tests. Durable job history
and geographic staging cleanup remain separate work.
