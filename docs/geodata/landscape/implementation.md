# Landscape quality overhaul

## Objective and delivery boundaries

Implement the user's map-quality design through the ordinary creator, immutable
package, shared scene and WebGPU/WebGL2/Canvas paths. This is not a competitive
symmetric generator. No economy, settlements, seasons or bridge building.
Source evidence describes circa-600 reconstruction limitations honestly; modern
natural-water geometry is evidence for a gameplay model, not ancient certainty.

Work initially began at `b65a5b3ae157ef7e1681a65275f566bd0e5b7232`, with a
clean tree. After the user requested the current integrated harness, map-only
changes were transferred without conflicts to `map/landscape-quality` on latest
`dev`, `b438dc8bca460a353ef66e4c7cc726568de89e5b`. The prior harness branch was
preserved. Its local commit hashes were not replayed over their merged equivalents.
The design investigation used `5e5bbb8759fe82eb0a6f44898c87d374b51bff21`.
The persistent task remains unfinished until new France visual and movement
qualification passes. France activation is a separate workstream.

## Ordered implementation

1. Baseline and corrections: freeze source identity and camera locations, remove
   forest parity, road-as-rock and material aliases; inventory private art.
2. Art foundation: review candidates, explicit terrain topology, three bounded
   2048-square atlas pages with page addressing and restoration accounting.
3. Landscape: `landscape_v2`, recipe 9, shared descriptor, coherent patch forests,
   clearing parcels, canopy floors, ecological species and resource approaches.
4. Geography: schema 10, independent layer descriptions, bounded window DEM
   reads, content-rooted sparse feature pages and supported river/lake vectors;
   continuous water rasterization, coast/beach treatment and protected water LOD.
5. Mountain/playability: slope/ridge exposure, temperate-summer elevation bands,
   authoritative cliffs and aligned reviewed dressing, feasible routes and fords.
6. Integration: new immutable France identity, fixed cameras on three backends,
   movement/replay, source memory and creator-path qualification.

New contracts must not activate incomplete behavior. Existing omitted detail
profile fields retain `standard_v1`; old schema/recipe/chunk readers remain.
Pin published behavior against explicit recipe IDs before advancing the latest
constant. No depletion overlay may be inferred across map hashes.

## Baseline identity

Private baseline manifest: `local-assets/maps-v8/` plus hash below; the matching
request is `local-assets/requests/france-30to1.json`. Neither private asset pixels
nor screenshots belong in Git.

- Content hash: `66c0b955473ac4e7cdcfd839dddf24b7346b6fb501f74d4c62d19fbdb501f442`.
- Schema 9, generator 9, recipe 8; seed 1; `standard_v1`, `circa600_v1`.
- Center 46.2 N, 2.2 E; effective square 1,200 km; 30:1; 20,000 tiles/axis.
- One game tile corresponds to 60 geographic metres.
- Investigation measured 128 samples/axis (9,375 m spacing), no typed river
  evidence, eight positive inland-water samples and two land elevation samples
  above 3,500 m. These are investigation metrics pending worker reverification.
- This package is a reproducible baseline, not proof of an earlier viewport.

## Frozen geographic camera targets

These are review targets, not claims that a source feature was retained.
Projected tile centers must be calculated using each package's actual CRS; do
not guess an equirectangular tile coordinate or shift a bookmark to hide defects.
Capture identical viewport, camera direction and normal/distant zoom settings
before/after, and record computed coordinates in private capture metadata.

| ID | Latitude N | Longitude E | Intended view |
| --- | ---: | ---: | --- |
| central_lowlands | 47.50 | 1.50 | Meadow/woodland composition |
| heavy_forest | 48.45 | 2.65 | Fontainebleau woodland |
| mediterranean | 43.60 | 5.10 | Dry scrubby grove landscape |
| atlantic_beach | 44.65 | -1.25 | Traversable dry beach and shallows |
| rocky_coast | 48.70 | -3.80 | Brittany rock/coast boundary |
| seine | 49.05 | 1.55 | Seine reach |
| loire | 47.25 | 0.45 | Loire reach |
| garonne | 44.25 | 0.30 | Garonne reach |
| rhone | 44.15 | 4.75 | Rhône reach |
| alps | 45.85 | 6.80 | Treeline, rock, snow and cliff bands |
| pyrenees | 42.80 | 0.15 | Ridge, valley and traversable pass |

Freeze zooms against the supported client range during initial capture rather
than assuming an unsupported numeric zoom. Missing/unrendered rivers fail the
river view; lakes are not a substitute. Bookmark visual qualification remains
unperformed while activation is unresolved.

## Budgets and acceptance

Do not relax the 512-chunk/128 MiB cache, 64 requests, 24,576 terrain triangles,
64 MiB instance buffers, backing-store limits or existing performance baselines.
Atlas base pixels: at most three 2048-square RGBA pages (48 MiB), two terrain
and one object/unit, at most 2,048 selected frames; account CPU/restoration copies.
Feature pages cover 256-square game tiles and decode to at most 1 MiB. Compact
chunks decode to at most 128 KiB. Overflow and missing required pages fail.

Forest fixtures require 16 seeds, boundary/parity and clearing cases, 85–95%
core density, at most 1% exterior occupancy and fewer than 1% eight-neighbor
singletons. Sparse/moderate/heavy/exceptional patch areas target 10–20%, 25–40%,
45–60%, 65–80% of suitable land before exclusions. Report actual post-exclusion
coverage and connected open space; sparse savanna is separate.

## Validation ledger

Record commands and outcomes for each stage; no synthetic pass qualifies France.
Mandatory handoff: exact revision and tree state, hooks, focused gates,
`make preflight`, assets, map/native/WASM/browser/E2E/performance/fuzz checks,
creator/geographic suites, actual activated source URL/hash for memory, reference
packages for movement, and unqualified external checks. No CI claim without
revision-bound CI evidence. Keep screenshots and original art private.

### Initial checks

- `make hooks-install hooks-check`: PASS at initial revision with clean tree.
- Initial `make map-test`: PASS (125 tests); the subsequent assertion capture
  failed intentionally to expose hashes, then hashes were pinned and the full
  `make fmt map-test` passed (125 tests). These are old-base fixture results.
- `AOE_MAP_PACKAGE=local-assets/maps-v8/<baseline-hash>.json make map-verify`:
  PASS offline; original France content hash unchanged.
- `make assets-verify`: PASS on the private pack, 20,396 frames / 27 pages.
- Old-base `make preflight`: cancelled on the user's base-correction request;
  not a pass. Updated-harness validation must be rerun on the new base.
- On updated `dev`, `make hooks-install hooks-check fmt`: PASS.
- On updated `dev`, `make map-test preflight browser-check`: PASS before the
  slope fix; map 125 tests, native Nextest 1,088 passed / 1 skipped, doctests,
  WASM build and synthetic smoke included. No independent-QA/CI authority claim.
- Read-only patch review found a procedural rock/ice/mud ramp-shading loss;
  all procedural ramp kernels were corrected, including snow, with alpha and
  single-round luminance fixtures. No recipe/geometry/ABI change.
- `make fmt build-wasm test-wasm test-e2e perf-ci fuzz-smoke` initially failed at
  nested Docker access before browser tests. An exact wider-access retry also
  failed. Host namespace reports socket group 65534; the harness container sees
  group 117. The documented `GID=117` environment setting resolves access without
  changing UID, socket permissions, gates, timeouts or performance baselines.
- `make GID=117 test-wasm test-e2e perf-ci fuzz-smoke`: WASM PASS (2 integration,
  48 client, 96 renderer cases, including actual GPU/Canvas pixels); E2E FAIL
  (9 failed, 8 skipped, 61 passed). Performance/fuzz were not reached in that
  command chain. Old cliff-color fixture expectations need semantic updates;
  E2E additionally exposed lost cliff-top-versus-skirt lighting, corrected by
  restoring .78 top / .72 skirt shading. Synthetic browser expectations now use
  independent dirt-rock and raw-water shore equations, with strict no-paving
  sentinels. Existing pixel-count, adjacency, parity, restoration and timeout
  floors remain unchanged. The failed idle-restoration metric included absent
  old palette classes; retained pixels showed restored geometry/textures.
- After cliff fix, two full WASM attempts aborted with runner `missing field
  chunk` after two GPU cases; no assertion failure was reported. Bounded the
  procedural pixel fixture to one device/atlas for all 13 probes and explicit
  device destruction; no probe/tolerance removed. Full WASM then PASS (146).
  This does not authenticate a root-cause diagnosis of the runner failures.
- Corrected full E2E: PASS, 70 passed / 8 existing source-only skips; includes
  real Canvas/WebGL2/WebGPU pixels, fallback, movement and context restoration.
- `make GID=117 perf-ci`: FAIL, optimized gzip WASM 228,902 against unchanged
  baseline 217,240 / 5% ceiling 228,102. All native instruction/allocation cases
  passed; fuzz was not reached. Isolated clean `b438dc8` with a preserved exact
  staged stash: unchanged `make GID=117 perf-wasm-size` measured 228,007 bytes.
- Restored exact stage-1 index, projected catalog startup fields at compile time
  (retaining source identity, role, frame count and topology; not unused prose),
  and used bounded role-range slots instead of a startup search tree. Error still
  explicitly identifies missing/corrupt reviewed archive and ID without Debug
  formatting. Native projection and browser BTreeMap-reference oracles added.
  `make fmt perf-wasm-size`: 227,152 bytes, below clean dev and existing ceiling;
  full performance/browser/native/parser gates must rerun before committing.
- Captures, full-sheet seamlessness, multi-atlas addressing, terrain topology
  propagation and source-backed France qualification remain open.
