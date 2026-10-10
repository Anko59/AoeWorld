# Landscape reference

## Objective and delivery boundaries

The map-quality design reaches players through the ordinary creator, immutable
package, shared scene and WebGPU/WebGL2/Canvas paths. This is not a competitive
symmetric generator. No economy, settlements, seasons or bridge building.
Source evidence describes circa-600 reconstruction limitations honestly; modern
natural-water geometry is evidence for a gameplay model, not ancient certainty.
France visual and movement qualification is still outstanding, and France
activation is a separate workstream.

## Versions before v1.0

There is no backward compatibility before v1.0. Each format has exactly one
current version, changed in place; other versions are rejected, not migrated.
After a format or generation change, regenerate local map packages and discard
client chunk caches.

| Contract | Current version | Any other value |
| --- | --- | --- |
| Map package schema (`MAP_SCHEMA_VERSION`) | 1 | Rejected by the strict reader |
| Generation recipe (`GENERATION_RECIPE_VERSION`) | 1 | Rejected; regenerate the package |
| Hydrology water model (`HYDROLOGY_WATER_MODEL_VERSION`) | 1 | Rejected |
| Compact chunk (`CHUNK_FORMAT_VERSION`) | 4 | Leading byte rejected |
| Gameplay and top-level protocol `VERSION` | 1 | No negotiation |
| Map job journal schema | 1 | Startup fails |

The strict package reader denies unknown fields and is the only reader. Every
environment field declares its own axis in its first pyramid level, and every
pyramid must end at one sample. Golden digests (chunk content, default package
hash and terrain, routes, start tiles) pin the current output as regression
guards, not as compatibility promises.

## Composed landscape

One composition is the only generation path, for source-backed packages and for
no-source fallback maps alike; fallback relief, water and biome noise is only
the base input to the same composition. Authoritative typed, scalar, chunk,
resource and blocking queries share that composer.

- Shared reservations: a starting glade, a seeded 192-tile opening lattice in
  which every cell has an opening, bent trails between openings, and direct
  start connectors to the eight surrounding openings. Reservations clear every
  appearance channel and procedural resources, never water or cliffs.
- Ecological patches: coherent jittered patches with sparse, moderate, heavy and
  exceptional densities. Sparse savanna is a separate mode.
- Historical parcels: coherent integer parcels realize HYDE crop and grazing
  over source-valid land, without biome, forest or placed-tree input. Combined
  fractions cap at 100, retaining crop first. Correlated clipping cannot promise
  exact source ratios; coverage evidence is retained and reported.
- Appearance: coherent canopy and forest-floor strengths, ecological palette,
  exposure and height band. Temperate-summer modeling uses source-height bands
  (1000/1800/2500/3500 m) and a 2300 m temperate/boreal/woodland treeline; it
  changes surface appearance, never heights, passability or provenance.
- Decorations are non-blocking and never become resources, blockers or
  economy objects.

Non-tree resources need a free cardinal neighbor satisfying the physical
height/passability rules, and dressing cannot refill crop, grazing, non-land or
reserved cells.

### Tree visual families

Source-backed tree families are part of generation. Raw potential natural
vegetation (PNV) classes map as follows:

| PNV class | Visual family |
| --- | --- |
| 8, 15, 17 | Conifer |
| 13 | Broadleaf |
| 9 | Coherent modeled 16×16-tile stands, 50:50 broadleaf/conifer |
| 14, unknown or missing | Biome fallback |

The biome fallback is Boreal → conifer, Tropical → tropical, Woodland/Savanna →
dry scrub, otherwise broadleaf. Non-tree resources carry the `Generic` family.
Families are semantic intent, not approval of conifer, scrub or tropical art;
the renderer falls back to approved broadleaf frames (see
[the rendering ADR](../../adr/0004-rendering.md)).

## Chunk wire

`/maps/{hash}/chunks/{x}/{y}` always serves one compact format, version byte 4.
`CompactChunk::encode`/`decode` take and return `LandscapeChunk`, with one
`CompactChunkError` type. All fields are little-endian; appearance is mandatory.

| Record | Bytes | Contents |
| --- | ---: | --- |
| Header | 7 | Version u8; tile, resource and decoration counts u16 |
| Tile | 35 | 20 terrain bytes (including 2 observation bytes), world x/y i32, canopy u16, floor u16, palette, exposure, height band |
| Resource | 22 | 21 node bytes plus visual family |
| Decoration | 11 | x, y, family, variant, orientation |

At most 1,024 of each record decode to 69,639 bytes, below the 128 KiB limit.
The decoder rejects invalid counts, lengths, enums, strengths and orientations,
duplicate or unsorted cells, and resources or decorations without a present
tile. The client cache holds only composed chunks.

## Starts and navigation

`search_start_checked(config, max_chunks, cancelled)` is the only start search.
Every start needs a clear 5×5 footprint, at least 256 reachable tiles, and a
bounded exit certificate (64 tiles, or toward the edge on small maps); see
[render polish](../render-polish.md) for the exact bounds. Exhausted bounds or
cancellation are not proof of absence.

Navigation has one behavior: equal-f ties prefer the greater actual cost
(progress priority). Walkability uses the single composed tile-and-node query.

## Server and creator

The default package directory is `local-assets/map-packages`. Pages are read
only as `{level}-{x}-{y}.json`. Source qualification accepts only the current
recipe. The web creator has no landscape-profile choice.

## Frozen geographic camera targets

These are review targets, not claims that a source feature was retained.
Projected tile centers must be calculated using each package's actual CRS; do
not guess an equirectangular tile coordinate or shift a bookmark to hide defects.
Capture identical viewport, camera direction and normal/distant zoom settings
before/after, and record computed coordinates in private capture metadata. The
France request is `local-assets/requests/france-30to1.json` (centre 46.2 N,
2.2 E; 1,200 km square; 30:1); regenerate its package into
`local-assets/map-packages/`. Neither private pixels nor screenshots belong in Git.

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

Freeze zooms against the supported client range during capture rather than
assuming an unsupported numeric zoom. Missing or unrendered rivers fail the
river view; lakes are not a substitute.

## Budgets and acceptance

Do not relax the 512-chunk/128 MiB cache, 64 requests, 24,576 terrain triangles,
64 MiB instance buffers, backing-store limits or existing performance baselines.
Atlas base pixels: at most three 2048-square RGBA pages (48 MiB), two terrain
and one object/unit, at most 2,048 selected frames; account CPU/restoration copies
(see [atlas design](atlas-design.md)). Feature pages cover 256-square game tiles
and decode to at most 1 MiB. Compact chunks decode to at most 128 KiB. Overflow
and missing required pages fail.

Forest fixtures require 16 seeds, boundary and clearing cases, 85–95% core
density, at most 1% exterior occupancy and fewer than 1% eight-neighbor
singletons. Sparse/moderate/heavy/exceptional patch areas target 10–20%, 25–40%,
45–60%, 65–80% of suitable land before exclusions. Report actual post-exclusion
coverage and connected open space; sparse savanna is separate.

Record commands and outcomes for each stage; no synthetic pass qualifies France.
A handoff states the exact revision and tree state, hooks, focused gates,
`make preflight`, assets, map/native/WASM/browser/E2E/performance/fuzz checks,
creator/geographic suites, the activated source URL/hash for memory, reference
packages for movement, and unqualified external checks. No CI claim without
revision-bound CI evidence. Keep screenshots and original art private.

## Source preparation

Overview packages always sample independent field axes
(`OverviewFieldAxes::LANDSCAPE`): elevation and year-600 history on 1024-sample
grids, potential vegetation and categorical water on 128-sample grids. The
server creator plan, the `map-generate` CLI and the worker JSON all carry these
four axes; the worker rejects out-of-bound axes and an elevation axis that does
not match `samples_per_axis` before any acquisition, and the server rejects a
worker package whose field axes differ from the request. The 1024 elevation cap
applies only to this field-local preparation; detailed preparation keeps its
coupled 128-sample overview context and its direct 128 cap. Field-local source
locks append `;axes=E/V/W/H` to their preprocessing identity.

Vector hydrology is selected explicitly, never by region: the creator's
`hydrology_mode: vectors` with an explicit overview, or
`AOE_MAP_HYDROLOGY_MODE=vectors` for the CLI. `PreparedHydrology::prepare_vectors`
uses the prepared overview context plus pinned HydroLAKES/HydroRIVERS, without
WorldCover catalog or raster work. Typed evidence and modeled water share the
1024 axis; categorical water stays at 128 and modern land cover is class-0
nodata, not an observation. The whole projected footprint must lie inside the
river-vector source coverage window less the query padding; the window, padding,
axes, mode and lock counts (7 overview, 2 vector) are defined once in
`aoe_map` environment/preparation and shared by geodata, worker and server.
Cancellation is checked before publishing the manifest; orphan immutable pages
are not a published package.

`make map-country-probe` (`source-country-probe`) reports ordinary start work and
four fixed 256 m local orders on one external 30:1, 1,200 km package. It keeps
limit, unavailable and cancelled outcomes distinct, never relaxes the canonical
50k/1:1 qualification case, and qualifies neither live activation nor hardware.
Its physical-blocker diagnostics, bounded connected-land observation, native
local orders, three-backend `make test-country-source` capture and one live
controller order are recorded in [country qualification](country-qualification.md).

## Open limits

<<<<<<< HEAD
Full-sheet seamlessness, expanded-art and shadow review, decoration drawing,
France capture/movement/memory, source-query cost, CI and dedicated-hardware
qualification remain open. Per-cell/page vector geometry scans have no work
counter; their worst case under current feature caps is unmeasured.
=======
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

### Opt-in integration qualification in progress

- Added explicit LandscapeV2/schema10/recipe9 dispatch; default StandardV1 stays
  schema9/recipe8/compact2. New content retains the original source geography key,
  including normalized correction-document request detail. Strict schema10 parsing
  rejects unknown nested fields; legacy parsing and invalid-recipe precedence stay
  compatible. No published package or geography is reseeded.
- Authoritative typed/scalar/chunk/resource/blocking queries share one composer.
  A lazy 25-cell stack memo reuses base/potential-resource samples, without eager
  fetching, shared mutable generator caches or world arrays. Cancellation is
  checked on cached hits. Start/route exclusions clear every appearance channel.
- Client cache retains explicit transport provenance, sparse coordinates, resource
  families and separate decorations, with all vector capacities accounted. Three
  rendering paths consume bounded palette/floor/canopy metadata in reserved sprite
  word pages.w; instance layout and budgets stay unchanged. Healthy broadleaf art is
  an explicit generic species fallback. Dressing remains undrawn pending art review.
- Integration preflight PASS: 1,197 native tests passed, one existing skip.
  The 82-tile sparse-edge HTTP fixture and all authoritative queries pass. A full
  tile/edge/diagonal-corner probe proves all eight starting routes clear and optimal.
  Equal-f/lower-g search exhausted the budget despite that connectivity. Recipe9
  now prefers higher-g on equal-f only; recipes3..8 keep their ordering, with
  unchanged heuristics, actual costs, node layouts and work/expansion budgets.
  The unchanged recipe9 start/route/movement/replay case PASSED in 23.263 seconds.
  The incremental-planner fixture now supplies page_samples=64 and passes its
  unchanged bounded-work assertion. Synthetic results do not qualify source movement
  or 20 Hz.
- Shared parsing/iterators, packed triangle metadata, canonical index sorting and
  bounded heap selection keep the optimized WASM within its budget. Feature/parity
  qualification is still pending; no renderer/feature removal.
- Real-browser gate PASS: two integration, 63 client and 116 rendering cases,
  including six compact3 cache cases, four bounded-heap/eviction reference cases,
  canonical ordering, three-page and V2 palette/floor/shadow pixels on GPU and Canvas.
  Buffer accounting checks the same owned chunk rather than a clone with different
  spare capacity. The missing test-only EntityId import was corrected. Preflight
  caught a build-wrapper production expect; replaced with ordinary error propagation.
  Performance CI and parser fuzz smoke PASS again after traversal reuse. Hook
  installation/check PASS. Recipe9 physical crossings use bounded BASE sampling;
  combined tile/node queries reuse one authoritative composition. Five new native
  parity/error tests pass, including legacy access order, provider recovery,
  cancellation, water/cliffs/height gaps and depleted overlays. Geometry, actual
  movement costs, forests and work/memory limits are unchanged.
  The exact no-provider Paris 512-tile request returned LimitReached in 13.188s:
  the unchanged certificate cannot establish a playable start. Its negative
  regression passes; no scout or flattened relief is fabricated.
  A separately labelled synthetic prepared 512-tile Temperate forest has complete
  constant-field pyramids, empty source locks and Fallback layer metadata. Runtime
  prepared-tile SourceDerived labels mean synthetic page input, not observations.
  The canonical disposable E2E server stages it alongside read-only validated
  existing packages in an ignored temporary root; private inputs are not overwritten.
  Four new native tests pass, including real production HTTP activation/provider
  reuse, valid start/clearance and authoritative movement. Native HTTP initially
  used a nonexistent activation suffix (405); corrected to POST /maps/{hash}.
  Browser cases initially omitted the documented replacement-world reconnect,
  then used a 35px target still inside the starting tile. Both test errors are
  corrected without production changes, assertions removed or deadlines increased.
  All six prepared play cases PASS: preferred/WebGL2/Canvas under both browser
  projects, compact3 metadata/chunks, actual scout pixels and authoritative
  movement across a certified tile boundary. Both creator cases also PASS.
  Initial full make test-e2e runs exceeded the unchanged 300s Docker deadline
  later in the 86-test suite. Traces showed progressing tests, not a frozen render.
  Repeated identical three-image 2048-square PNG fixture generation measured 951ms
  for one page setup. Worker-local caching now retains only generated manifest/PNG
  outputs; local-pack probes and routes remain per call/page. Static Git diff proves
  texel/manifest generation unchanged. A new cached/uncached consistency test checks
  every PNG byte and manifest plus forced/unforced per-page routing. This is not an
  independently frozen pre-refactor byte oracle. All assertions, diagnostics,
  browser projects and deadlines remain unchanged.
  Full make GID=117 test-e2e now PASS: 79 passed / 8 existing source-only skips,
  87 cases in 4.1m, including six prepared play cases and both creator cases.
- Schema10 validates and samples field-local DEM/water/PNV/history axes using
  existing pyramid metadata: no new wire fields or page roots. Legacy coupling,
  validation precedence and the published 16,384-sample ceiling remain unchanged;
  future country acquisition limits are separate, not a global ceiling reduction.
  Seven new native tests PASS, covering 1024/128/128/1024 and odd partial pages,
  canonical complete pyramids, strict roundtrips/identities, nodata, cancellations,
  provider/eager parity, missing/duplicate/outside pages and legacy maximum axes.
  Early preflight caught three incomplete positive modeled-water fixtures and an
  accidental public ceiling reduction; completed the fixture pyramid and restored
  the ceiling, retaining all rejection/budget assertions. Browser-check PASS after
  replacing nullable token/hash assertions with explicit guards and formatting.
  These integrated changes are committed as bafa16043a58232cbaaf56df9222c2e967972041.
  No active/qualified France map, finished visuals, real-source movement/memory,
  CI or dedicated-hardware claims.
- Follow-up source preparation adds explicit OverviewFieldAxes options and native
  worker JSON handoff, retaining absent-option legacy coupled behavior. Landscape
  overview uses DEM/history 1024 and PNV/water 128; server creator planning and CLI
  hand off all four axes, report DEM spacing, and reject worker field downgrades.
  Mixed Standard fields, mismatched elevation counts and wrong correction axis/year
  reject before acquisition. Direct legacy DEM/PNV/water ceilings remain 128;
  the high DEM cap is profile-aware/opt-in, not a global source-cap increase.
  Explicit preprocessing binds the selected axes; legacy strings stay unchanged.
  Offline geodata gate PASS: 200 library, one binary and six integration cases,
  including real varied GDAL 1024 elevation, nodata, complete field pyramids,
  independent correction grids and legacy page/root comparisons. An initial
  vector-flow assertion used a terminal reach; added a genuinely connected third
  reach only to that new fixture, retaining East flow and asserting terminal
  Unknown flow. Existing legacy fixture/production rejection logic stay unchanged.
  New test leaf belongs under an explicit tests directory for policy classification;
  production unwrap rejection and all strict lint checks remain enabled.
  Opt-in PreparedHydrology::prepare_vectors uses supplied overview context plus
  pinned HydroLAKES/HydroRIVERS, bypassing WorldCover catalog/HEAD/raster work.
  It conservatively validates projected cell vertices/midpoints/centers inside the
  existing Europe pilot, checks cancellation and malformed context before cache,
  and retains mapped extents as modeled evidence, not observations at year 600.
  Modern pages are class0 nodata, with explicit not-requested preprocessing;
  the required 2021 field is a classification legend, not coverage evidence.
  Offline vector fixtures exercise lake/river/regulation/topology/page roots;
  successful real acquisition and source-lock counts are not qualified by mocks.
  This slice is committed as c4871452a788b6e64c7e7f092eddda28028ee5cd;
  final preflight passed 1208 native tests/one skip, browser 79 PASS/eight existing
  source-only skips in 4.5m, hooks/perf/parser fuzz PASS. Seven actual overview
  inputs verified (6043158637 cached bytes, zero new download bytes). Separate
  immutable France LandscapeV2 candidate c3436bbd6fd154971bbf79ec6f3b88669123e09d2df41e534fa718f83793e8e6
  generated and map-verify PASS; old baseline untouched. Candidate has no vector
  evidence and is not activated or gameplay/hardware-qualified.
  Follow-up routing selects vectors explicitly via native hydrology_mode:vectors,
  Creator overview/LandscapeV2 hydrology_mode:vectors or CLI configuration
  AOE_MAP_HYDROLOGY_MODE=vectors (forwarded into Docker). Absent/none preserves
  the original path; profile alone never enables a global or European default.
  Profile, exact axes, corrections and full pilot footprint reject before overview
  acquisition. Composition retains categorical water128 and attaches independently
  typed modeled water/evidence1024, nine real locks and class0 modern nodata.
  Raw wire 2021 is legend-only; no public per-layer source-lock DTO/WorldCover lookup
  exists here, so no fake modern observation metadata or lock is introduced.
  Cancellation is checked before publication; acquisition inside the legacy
  overview helper and a narrow check-to-publication race remain explicit limits.
  Shared worker options (field axes, hydrology mode, the 1024 landscape axis, the
  river-vector source-coverage window and named overview/vector lock counts) live
  once in `aoe_map` environment/preparation; server and worker consume them, and
  a geodata test pins the lock counts to the documented source lists.
  Follow-up focused tests PASS: 204 geodata library, one binary, six integration.
  Full preflight PASS: 1216 native tests/one existing skip. Hooks, perf and fuzz
  PASS; complete browser suite 79 PASS/eight existing source-only skips in 3.8m.
  Initial fixture failures exposed wrong fixture module/borrow and full-pyramid
  DEM context; fixed by selecting real level-zero pages without lowering1024
  or relaxing the old strict ElevationGrid parser. Test paths/Clippy policy retained.
  Real explicit-vector France candidate 146c49268ecac72372db928436976fcc7ac96a4350d81dbd3a75b983d69c3b50
  generated from nine verified cached inputs (6873327368 bytes, zero download)
  and map-verify PASS. 1228 indexed pages, typed evidence/model present.
  Ordinary start found at (9999,9999), but zero of four fixed256m local orders
  produced a path: east/south/north invalid destination, west budget exceeded.
  Nonvector candidate had east/west paths129tiles, south invalid, north budget.
  No budgets raised, geometry cleared, fake positive substituted or activation
  performed. These negative route results require source/model/traversal diagnosis.
  A read-only sampler audit found worst-case per-cell/page-feature geometry scans
  can reach billions of predicates under current feature caps; no page work counter
  or exact generation-phase timing exists. Actual generation finished successfully;
  this is an optimization/measurement risk, not a claimed hang or timeout.
  Separate map-country-probe diagnoses ordinary start work and four fixed 256m
  local orders without weakening the canonical 50k/1:1 source qualification case.
  It preserves limit/unavailable/cancelled outcomes and original endpoints/budgets.
  Follow-up physical/resource observations and bounded4096tile components found
  no water/cliff obstruction on the typed512 fixed-line samples: tree resources
  explain the observed effective blockers; planner budget failures remain distinct.
  Separate four predeclared64m native orders retain every result, without replacing
  old256m probes. TypedS/W and overviewE/S/N arrived in authoritative simulation.
  Country qualification ledger records exact counts/limits; this is not live
  activation, global traversal, measured realtime20Hz or hardware qualification.
  Negative country results remain evidence, not reasons to relax physics/budgets.
  Final preflight/browser gate results for this follow-up are reported separately;
  offline PASS is not real-source qualification. Real France preparation is now
  verified, but activation, global source traversal, visuals, memory and hardware
  remain pending. Neither successful start search nor immutable verification
  makes the typed-vector candidate ready to activate.
>>>>>>> 6ed9e2d (test(landscape): qualify source-backed landscapes on server, renderers and controller)
