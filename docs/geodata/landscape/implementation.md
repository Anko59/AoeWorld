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
  subsequent full performance and fuzz smoke PASS. Full WASM PASS (147), E2E
  PASS (70 / 8 source-only skips), preflight PASS (1,089 native plus doctests,
  static/dependency/WASM-build/smoke). One preflight preparation rejected changed
  source; exact retry succeeded, root cause unassessed. Hooks and indexed static
  validation PASS. Foundation committed `c658a8354da06b59db450c7a54601f40be897954`;
  working tree clean immediately afterward. No CI/source/hardware qualification.

### Multi-atlas working implementation

- Pure packer validates count (2,048 max) and all extents before placement
  allocation/sort, fixed 4 KiB scratch, deterministic shelf first-fit with
  transactional cursor trials. Terrain uses pages 0/1; all objects and units use
  page 2. Overflow fails; semantic frame order, masks and signed anchors survive.
- Page-major 50,331,648-byte loader decodes source pages sequentially, uses bounded
  semantic-index source grouping, and rejects malformed present optional art.
  Explicit catalog topology now reaches shared scene selection; 100 frames alone
  no longer imply a periodic sheet. No additional art has been promoted.
- Shared page/UV addresses carry all three terrain samples. Sprite ABI is 112
  bytes, selectors at byte 96. WebGPU D2Array and WebGL2 integer attribute/array
  texture preserve one ordered draw; Canvas sampling uses the same addresses.
  Diagnostic texture remains one 8-square page / 256 bytes, not three game pages.
- WebGL owns one additional 48 MiB restoration source, reused without cloning on
  restoration. Canvas retains one 48 MiB source plus zero to three lazy 16 MiB
  legacy canvases and presentation buffers. GL/Canvas GPU counters remain
  unavailable through the existing API; 48 MiB is not total process memory.
- `make fmt build-wasm`: PASS. `make GID=117 test-wasm perf-ci`: PASS (2 integration,
  53 client, 108 renderer). Actual pixels cover equal UVs on different pages,
  three-page blends, mirrored page-2 bodies/shadows, depth ties and restoration;
  restoration unit fixture invokes the actual callback, not a real loss event.
  Optimized gzip 227,958, below unchanged 228,102 ceiling. Subsequent browser-check
  PASS; E2E PASS (70 / 8 source-only skips); preflight PASS (1,101 native including
  12 packer cases / 1 existing skip, plus doctests/static/WASM-build/smoke); fuzz
  smoke PASS. Module naming/attribute layout and integer-equivalent gutter lint
  corrections resolved strict policy failures, without changing gates. Strict
  snapshot preparation also rejected changing inputs; exact retries succeeded,
  root cause unassessed. Final WASM rerun and hooks PASS. Indexed commit
  `0a467a6ff12f32ea4f84404f60f65a07a37bff2d`; tree clean immediately afterward.
- Full-sheet seamlessness, expanded-art/shadow review, source activation, France
  capture/movement/memory, CI and dedicated hardware qualification remain open.

### Ecological patch precursor (not map activation)

- Base terrain sampling separated from legacy appearance decoration in dense and
  fallible provider paths. Future shared-mask neighbor callbacks cannot recurse
  through decorated tile/object queries. Four regression cases and published
  recipe 3–8 semantic goldens PASS; heights, provenance, water, passability,
  resources and error/cancellation forwarding unchanged. `make fmt` PASS;
  undocumented `test-native` target rejected and replaced with documented gates.
  `make GID=117 preflight perf-ci fuzz-smoke` PASS (1,105 native / 1 existing skip,
  doctests/static/WASM-build/smoke, unchanged budgets). Hooks/indexed validation
  PASS after strict snapshot retry. Commit `9e0aad2211a52e1c099d1ea824d1f4a5d333d2d8`.
- First analytic candidate met numeric area/occupancy but review rejected its
  regular equal-radius macrocell pattern. Revised bounded library uses jittered
  centres/radii, asymmetric overlapping lobes/eight orientations, coarse cluster
  propensity and smooth integer warp. No allocation/world-size array/float/unsafe;
  world-key/seed stable, 25-cell lookup. Shared masks remove canopy AND floor;
  fallible neighbor callbacks preserve caller errors. Sparse savanna is separate.
- Standalone pinned Docker Rust compilation/execution PASS (9 tests, 33.69 s),
  then integrated library `make fmt` and `make GID=117 preflight perf-ci fuzz-smoke`
  PASS (1,114 native / 1 existing skip, plus doctests/static/WASM-build/smoke,
  unchanged budgets). Explicit test paths fixed a Rust alias-path collision with
  legacy landscape tests; no assertions removed. All 16 seeds on 256/512 native
  homogeneous suitable-land fixtures meet sparse/moderate/heavy/exceptional
  area 10–20/25–40/45–60/65–80%; core 85–95%; exterior <=1%; singletons <1%.
  Exact mirror counts, seams/query order, parity classes, source-mask fractions,
  integer extremes, error forwarding and parameter bounds pass actual Rust.
- These are analytical fixture results, not geographic composition acceptance.
  Source crop/grazing denominator remains SOURCE VALID LAND, distinct from forest
  or ecology-suitable area; post-mask metrics report both original and eligible
  domains. Actual source masks/eco-region transitions, routes/resource approaches,
  shape/pixel review and runtime cost still require integration and measurement.
  No profile/recipe/schema/package activation: published defaults remain unchanged.

### Shared historical/physical masks and source evaluation

- Coherent integer historical parcels now realize crop/grazing over source-valid
  land, without biome/forest/placed-tree input. Individually invalid percentages
  reject; combined fractions cap at 100 retaining crop first (explicit model
  policy, not a repair of observations). Integer area-preserving shears bend
  16-tile cells; locally oriented cyclic Hilbert intervals and coarse ranking
  avoid pixel clearing. All 16 seeds on native 512-square homogeneous fields
  pass fraction tolerance 2 percentage points (0/25/50/100, mixed 20+30, grazing
  100), spatial coherence, neutral-domain/forest-subtype separation, deterministic
  seams/extremes and bijection checks. Correlated clipping explicitly cannot
  promise source ratios; evidence/partial coverage must be retained and reported.
- Raw dense/provider historical observations expose all six coverage bytes,
  crop/grazing/population without dropping valid-land-zero cells. Legacy wrappers
  retain the exact old filtering, coordinates, values, load ordering and source
  errors. Five raw observation cases and recipes 3–8 semantic goldens PASS.
- Shared ecological assessment uses undecorated water, passability/surface and
  material suitability plus one history/routes/resource-approaches/start mask;
  canopy/floor/trees clear together before singleton filtering. Explicit
  region/support are model policy, not modern-cover or inferred PNV evidence;
  no-data/unobserved history status survives. Savanna remains a separate sparse
  mode. Support >1000 rejects. Five assessment cases PASS.
- Explicit `evaluate_landscape_with_cancel` queries dense or lazy BASE source
  inputs with that same neighbor mask; raw historical coverage remains in the
  response. Missing/corrupt pages/cancellation propagate, outside bounds returns
  none; three query cases PASS. Queries demonstrably do not mutate published
  tile/resource/chunk results. This is candidate evaluation, not active gameplay.
- `make fmt` and `make GID=117 preflight perf-ci fuzz-smoke`: PASS (1,134 native /
  1 existing skip; doctests/static/WASM-build/smoke and unchanged performance
  budgets). Initial fixture lint corrected without allowance; a nodata fixture
  wrongly assigned positive population and was rejected by unchanged page
  validation. Fixture corrected to zero pressure for valid-land-zero cells;
  no source validation or assertion weakened. Test sidecars use recognized paths.
- Limits: 64-tile local motifs/directional shear and cyclic interval splitting
  need shape review. Heterogeneous fields/partial coverage and geography require
  area-aware qualification; candidate query/page-cache cost is not measured by
  the existing performance scenarios. No activated profile/recipe/schema/compact
  format, new source package, France captures/movement/memory, CI or hardware
  qualification. Before activation, pin old StandardV1 generator/schema/recipe
  hash inputs: advancing latest schema aliases changes generator identity, and
  current geography hashing includes detail. Preserve schema9/recipe8 identities
  and reject mixed old/new profile-contract combinations explicitly.

### Candidate scene contract and compact format 3

- Typed landscape scenes now carry explicit tile coordinates/base terrain,
  paired integer canopy/floor strengths, ecological palette, exposure/height
  bands, resource visual families and separate nonblocking decorations. Existing
  `Tile`, `ResourceNode`, `Chunk` and compact 1/2 bodies remain unchanged. Species
  families are semantic intent, not approval of conifer/scrub/tropical artwork.
- New explicit candidate chunk query produces source-based floors/trees with
  one pre-singleton mask including routes/starts and potential resource clusters'
  full adjacent approach ring. Non-tree resources require a free cardinal base
  neighbor satisfying existing physical height/passability rules; no decorated
  callbacks or tree-only removals. Dressing cannot refill historical crop/grazing/
  nonland or reserved cells. Default published dispatch remains unchanged.
- Explicit temperate-summer modeling uses source-height bands (1000/1800/2500/
  3500 m) and temperate/boreal/woodland treeline at 2300 m. It changes candidate
  surface appearance, never source heights/corners/passability/provenance. These
  are provisional model policies, not local-climate or France qualification.
- `encode_landscape`/`decode_landscape` implements strict compact3: 7-byte header,
  37-byte tiles (20-byte old base + explicit coords + 9-byte metadata), 22-byte
  resources and 12-byte decorations. Maximum 1024 tiles + 1024 resources + 1024
  decorations is 72,711 decoded bytes, below unchanged 128 KiB. Reject invalid
  counts/lengths/enums/reserved bytes/strengths/orientations, duplicate cells,
  unsorted/outside tiles and resources/dressing without a present tile. Savanna/
  treeless modes require zero forest strengths. No parallel extension arrays.
- Explicit coordinates fix new sparse/partial-edge representation; 18-wide rows
  are NOT silently treated as 32-wide. Legacy projection keeps implied original
  coordinates without extra parser rejection; client must retain wire-format
  provenance and its existing world-aware legacy chunk-width layout. Appearance
  none is allowed in compact3, so it cannot identify the original format.
- Old `decode` rejects3 rather than discard metadata; new reader projects1/2
  with no inferred appearance/species/dressing. Twelve codec cases and five
  source/candidate cases PASS, including dense species/floor coherence, unchanged
  published chunks, geometry/provenance, shared exclusions, sparse edges/order,
  summer treeline and separate savanna. Valid v2/v3 ASCII-hex fuzz seeds reach
  their real readers; existing binary/name corpus entries are retained verbatim.
- `make fmt`; `make GID=117 preflight perf-ci fuzz-smoke`: PASS, 1,152 native /
  1 existing skip, doctests/static/WASM-build/smoke and unchanged performance
  budgets. Initial alias-loaded test module corrected to explicit path (no lint
  allowances); an isolated snapshot preparation reported source changed and the
  verified frozen staged inputs were retried successfully (root cause unassessed).
- Limits: inactive candidate contracts; client/shared-scene/backend consumption,
  profile/package/creator dispatch and opt-in UI still required. New source-query
  cost/real resource access and open-space composition need dedicated evidence;
  no source France activation, captures, movement, memory, CI or hardware claims.
