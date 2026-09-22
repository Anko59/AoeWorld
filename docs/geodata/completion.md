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
| M4 | Physical movement and long routes | Physical speed and bounded detours implemented; 100 km qualification pending | Fractional/waypoint distance carry; resumable fair search; lazy connectivity and detours; slope consistency; 100 km travel/replay |
| M5 | Resource lifecycle | Persistence and synchronization verified; source-session qualification pending | Independent ore streams; obstruction-aware access; bounded resource deltas; persisted overlays; eviction/reconnect/reload |
| M6 | Terrain and resource rendering | Art/stale-frame fixes verified; geometry pending | Reviewed missing art; ramp/cliff/water meshes; transitions; surface picking/occlusion; covering LOD; bounded requests; both backends |
| M7 | Creation experience | Creator, recovery and measured progress verified; submission recovery pending | Accurate estimates/detail; useful preview; create/cancel/retry/open; atomic activation; bounded job retention/recovery |
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
not allocator-inclusive RSS. The focused long-route follow-up replaces the
earlier four-portal routing with resumable exact sparse A* and retains a proven full
route continuation across 32-tile movement segments. It charges queue pops,
neighbor probes, parent hops, and route conversion steps; caps one search at
12,800,000 work units and 524,288 logical
retained planner entries across search, reconstruction, and continuation, and
limits a world to 64 planners with at most two heavy searches. A fixed
4,094-tile straight corridor crossed 128 movement
segments in 45,026 work units and 32,756 peak entries; a 62-tile wall detour
through its only gateway used 19,207 work units and 2,825 entries, while the
matching sealed wall was proven unreachable in 19,394 work units and 2,084
entries. Straight and detour work includes one unit per parent hop and route
conversion step. Map-test passes 66 tests. These synthetic fixtures
establish exact continuation and boundedness for their cases, not source-backed
100 km behavior or RTS latency. At 16,384 shared route-work units per tick,
the 12.8 million per-search cap could consume up to 782 fully allocated ticks
(about 39 seconds at 20 Hz); this is a budget bound, not a measured route
delay. A 100 km order spans 50,000 two-meter tiles and remains a separate
qualification target.

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

Earlier revision-bound evidence is preserved in
[completion history](completion-history.md).

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

M3 regional evidence acquisition is reviewed in
[PR #32](https://github.com/Anko59/AoeWorld/pull/32), revision
`3879327379c0911d6ff4bdabde3b188eb8675eff`. WorldCover, HydroLAKES, and a
bounded European HydroRIVERS pilot feed detailed preparation. Exact ranged
responses and cached digests are verified; archive access uses GDAL's braced
VSI syntax. Typed classes are still preparation intermediates, so this does
not close historical reconstruction or typed terrain consumption. Independent
review, hooks, native tests, preflight, and actual Paris generation/verification
passed. Integration `0ca2cd174ef397b20fe5538441bf384988942a7f` passes 367 native
tests. Dockerized `make map-generate-detailed` and `make map-verify` generated
and verified recipe-3 Paris at 1024 samples per axis, hash
`718664fc84d93df87e0ab3e47f706b5a3339e4a27570000ab66642029e7882ac`.

M7 durable history and browser recovery are reviewed in
[PR #33](https://github.com/Anko59/AoeWorld/pull/33), revision
`09b0dcc9c09d3adfc12584148c0fe9a16bcdfa6b`. Bounded atomic history preserves
original requests and monotonic IDs; interrupted work becomes explicitly
retryable. Accepted browser jobs survive polling failures and reload without
another POST. Checkpoint failures preserve live state or fail queued work
explicitly. Valid immutable package artifacts can still be discovered at startup
independently of failed job records; activation remains explicit. Independent
review, hooks, preflight (356 native), browser-check, E2E (27), and clean
pre-push passed. Combined integration `b9e41bd91880c4aecc9c6ec533231a3edb94d8d7`
passes preflight and E2E (27). Lost POST acknowledgements before the job ID is
received, measured progress, and cancelled staging cleanup remain open.

M4 bounded detours and route reconstruction are reviewed in
[PR #34](https://github.com/Anko59/AoeWorld/pull/34), revision
`53c518477505c77e2dd5164470088f06f5acfe84`. Exact sparse A* replaces the incomplete four-portal search. Parent
backtrace and conversion share the tick work budget; continuations retain the
proven route between movement segments. Replay hashing uses fixed-width values.
Independent review, hooks, map-test (66), preflight (334 native), and clean
pre-push passed. Combined integration `76154946d012579efbee922d0bb6ddab6c1d0db3`
passes map-test and preflight (380 native). Source-backed 100 km movement and
runtime qualification remain open; synthetic corridor results above are not
that evidence.

M7 leased worker scratch is reviewed in
[PR #35](https://github.com/Anko59/AoeWorld/pull/35), revision
`d64de5f8e8dfa4ef08cb9110051960c443fe1cb3`. Server and worker leases protect
active detailed pyramid staging; startup and the next preparation recover
abandoned scopes. Normal completion and cancellation clean up after process
reaping. Registry locking serializes recovery/removal, and Unix permissions
restrict scopes to their owner. Independent review, hooks, preflight (382
native), and clean pre-push passed. Combined integration
`9ce626b58aac1f38a976933e42d85c5e9c0c8ff3` passes preflight (388 native).
Provider extraction temporaries, incomplete immutable publication files, and
direct CLI staging are outside this cleanup contract.

Coverage at integration `adda0eec37c3197d2bf0e49e4b100426173f6dbf` is
15,419/19,566 production lines (78.8%): the unchanged 85% overall floor fails.
All critical groups pass and no sources are missing. This supersedes the older
79.9% measurement for the expanded implementation; qualification remains open.

M8 cache and worker boundary qualification is reviewed in
[PR #36](https://github.com/Anko59/AoeWorld/pull/36), revision
`26e095076be3cfdcea2b81a3b651a91b0bb6563b`. Cache metadata reads are bounded,
recovery requires canonical content-addressed names, and real worker CLI tests
cover offline validation, package verification and a GDAL raster fixture.
The WebSocket diagnostic test now uses a total deadline while preserving every
protocol assertion. Independent review, hooks and preflight passed. Coverage
on this feature revision is 15,848/19,589 lines (80.9%); all critical groups
pass with no missing sources. The unchanged 85% overall floor still fails.

M6 byte-exact bounded browser PNG decoding is reviewed in
[PR #37](https://github.com/Anko59/AoeWorld/pull/37), revision
`471e8ff88155f1fdc25306454255a405e64fe0df`. Browser deflate streams replace the
reachable WASM inflater while retaining explicit PNG validation, all filters,
Adam7 and exact partial-alpha colors. Compressed writes and inflated reads are
bounded with backpressure. `DecompressionStream("deflate")` is now required.
Hooks, preflight, WASM (18), standard E2E (23) and perf-ci passed on the feature
revision: 198,834 gzip bytes against the unchanged 217,240-byte baseline.
Actual private-pack decoding/verification passes, but two existing E2E terrain
color heuristics fail against real artwork; this is not an art qualification
pass. Combined integration `4c3c385432c24522f462bc093d93e921f8c21796` passes
preflight (398 native tests), test-wasm (18), test-e2e (27), and perf-ci.
These are local Dockerized checks; no remote CI or hardware qualification is
claimed.

M7 measured preparation progress is reviewed in
[PR #38](https://github.com/Anko59/AoeWorld/pull/38), revision
`52947e8672787e5b96f59e4d703c139a15bf68e2`. Download bytes and persisted
four-layer pyramid pages are phase-local measured counters; sampling,
publication and verification have named phases with absent unknown totals.
The creator no longer displays fixed 5%/10% placeholders. Bounded atomic
telemetry is optional, and both overview/detailed workers retain scratch leases.
Independent review caught and verified the overview lease fix. Hooks, preflight
(392 native), E2E (29), and clean pre-push pass. Combined integration
`3f57de0223878ad3ebbaf5d97d971a93aae06168` passes preflight (402) and E2E (29).
No overall time estimate or full-product qualification is inferred from these
counters. The combined PNG size at `4c3c385` was 198,336 gzip bytes, passing the
unchanged 217,240-byte baseline; the earlier feature count remains revision-bound.
