# Live-preview follow-up

User observations on the opt-in source candidate at revision `d2c1020`:
streaming stalls, uniform regional forest density, absent mountain snow,
rough ground transitions and zoom aliasing, absent continuous rivers,
repeated square lakes, gray coast bands, missing beaches, coarse minimap,
and undersized resources. These observations remain acceptance work, not
qualification claims. France is external source test data, never generator logic.

## First corrective stages

1. Reuse lazy undecorated terrain/resource candidates within one bounded chunk
   halo instead of rebuilding overlapping five-square point halos 1024 times.
   Preserve exact output, query order, error propagation and cancellation;
   standalone point queries keep their bounded memo. No global terrain cache.
2. Stop the renderer's LandscapeV2 vegetation path from replacing authoritative
   Dirt with grass merely because palette/floor metadata is present. This is a
   renderer correction, not a dirt coverage adjustment or finished blending.
3. Implement shared-edge transition fields/masks and zoom minification with
   cross-backend pixel, seam, atlas-bleed and memory tests. Face-constant floor
   strengths, fixed nearest level-zero sampling, and stretched coarse-cell
   frames are separate problems; whole-atlas mipmapping is not a safe shortcut.

## Measurements before changes

Three sequential cold HTTP chunk requests on the existing `d2c1020` preview:
`(420,310)` 168.218ms/90560 bytes, `(421,310)` 146.425ms/100696 bytes,
`(422,310)` 134.448ms/91120 bytes. Responses retained outside Git under
`reports/streaming-baseline-d2c1020`. This small local sample is not a camera
cadence, load/concurrency, canonical source-memory or hardware qualification.
With the source-provider registry initialized first, the optimized empty chunk
cache served those same coordinates in22.065ms,24.241ms and21.428ms respectively.
All three JSON responses were byte-identical to the originals. This local sample
is roughly6–8× faster for uncached chunks, not a proven camera/frame-rate gain.
A completely fresh optimized server's first request also paid source-registry
initialization (350.203ms including chunk420,310 in one run;1275.097ms including
start-region chunk312,312 in another); that startup/activation work is not fixed
by this memo.

## Follow-up appearance and request stage

Forest-floor strengths now gather at shared displayed world vertices and
interpolate through Canvas, WebGPU and WebGL2 with a tagged reserved packet word.
The112-byte Sprite layout and legacy/manual constant-floor fallback remain pinned;
see the rendering ADR for exact field/storage accounting. This smooths floor
strength only, not palette/canopy discontinuities, nonperiodic accent seams or
zoom minification. Gradient/shared-edge/order/fallback pixels run on all backends.

Camera demand uses a bounded64-request window with per-request identity and
AbortController signals. Retired success/error cannot remove a replacement slot,
insert stale terrain or trigger the normal5-second error backoff. Cache reset
aborts pending requests without resetting identity. If abort support is unavailable,
obsolete requests retain slots until completion rather than dropping accounting.
Native race/cap tests and real browser tests hold an entire old64-request window,
jump the camera, and verify new terrain arrives before the old window is released,
with at most64 live non-aborted signals and connected status preserved.

Additional user review confirmed the chunk optimization improved streaming and
average-zoom elevation rendering works. Remaining priorities include zoom-out soil
filtering (currently obscures relief), a cleaner grid overlay, varied tree types,
and reviewed cliffs/boulders/rocks. Larger lakes are still square and unbeached.
Existing private original images support reviewing conifer4654 and palms4653;
family/shadow/runtime/packing promotion is separate work, not implied by names.
Authored cliff sources contain placeholders and directional/anchor hazards;
use on existing physical faces needs placement/depth qualification. No source
pixels or local inspection captures enter Git or public build artifacts.

Focused follow-up verification (working tree based on a0c29cdc):

- `make GID=117 browser-check test-wasm perf-ci fuzz-smoke`: pass;
  actual WASM tests2 core/63 client/124 rendering, including all3 backend pixels.
- Real Playwright camera cancellation regression:2 projects pass against8081;
  intentionally held old requests remain unreleased until new terrain progresses.
- Same pinned actual-source/asset inputs with `make GID=117 test-country-source`:
  pass, all3 backend observations have empty error lists. Local evidence is in
  ignored `reports/country-source/run-6szgPm/`:498 unique captured chunks,
  Chromium RSS peak947,974,144 bytes, final-four spread3,354,624 bytes. Cache reaches
  its512 count, but `eviction_limit_exercised=false`; this is not canonical50k,
  1:1 global-traversal, memory-eviction or dedicated-hardware qualification.
- Initial gzip WASM234,350 bytes failed the unchanged217,240+5% cap228,102.
  Bounded request-vector scans, canonical once-per-vertex quantization and removal
  of an unused duplicate discovery set reduced measured gzip to226,052: pass.
  Eviction keeps its exact old comparator; preferred membership uses a sorted
  compact vector, with stable-reference tests including unsorted duplicate inputs.
  No deadline, source identity, baseline, atlas, instance or cache cap was relaxed.

The appearance/request stage is committed as
`796360cbd4d526a6c0ac09ad8d291564427c5a53`. Required hooks and corrected preflight
passed1239 native tests with1 existing skip; an initial test-module attribute-order
policy failure was fixed in the declaration, not in the policy. The unchanged full
`make GID=117 test-e2e` now passes81 tests with9 environment-specific skips in3.9m;
the earlier timed-out run remains a failed historical attempt, not a prior pass.
The refreshed8081 preview reports this exact revision and its separate explicit
original-source three-backend capture passes (local evidence under ignored
`reports/france-live-preview-796360c/`). External memory/hardware limits still apply.

## Zoom soil follow-up

The V2 forest bed now selects coherent periodic dirt rather than rendering
nonperiodic15011 forest accents as full-floor sheets. Legacy None/material6 keeps
its exact original art selection. Missing dirt leaves the primary alone rather
than silently promoting an accent. This is presentation policy only: generator
bytes, resources, topology and source identity are unchanged. The shared floor
field still only smooths floor strength, not canopy/palette ownership.

A bounded V2 vegetative terrain-only minification kernel is the next
implementation: preserve original center alpha/coverage, alpha-weight and
byte-round four rect-clamped RGB taps only when the source footprint is minified.
Legacy None/manual zero-word/protected faces and all sprites retain nearest
sampling. No whole-atlas mip allocation and no claim that four sparse samples
solve severe zoom or coarse-LOD world-frequency stretch. Validate actual
three-backend contrast, transparency, atlas/page-edge and depth pixels and the
unchanged WASM budget. Initial testing caught an unintended legacy filtering
change; restore that exact old reference contract rather than weakening its test.

Focused V2 filter verification on the working tree based on796360c:

- `make GID=117 fmt test-wasm perf-wasm-size`: pass after fixing explicit
  nested-module paths, restoring exact legacy nearest behavior, correcting unused
  single-layer page assertions and using a real two-address floor-packet fixture.
  Actual WASM tests2 core/63 client/128 rendering. New Canvas tests compare the
  entire depth buffer bit-for-bit with identical unfiltered geometry; no tolerance
  was introduced. Rect-edge tests use a wider synthetic sheet with robust interior
  coverage, retaining exact red-neighbor rejection rather than lowering thresholds.
- `make GID=117 browser-check perf-ci fuzz-smoke`: pass. Optimized gzip227,948
  against unchanged cap228,102/baseline217,240+5% (154 bytes remaining). No budget,
  profile, deadline, baseline or hook/test policy was relaxed.
- Same pinned sources with `make GID=117 test-country-source`: pass; all3 backend
  observations have empty errors. Ignored local evidence `reports/country-source/run-EDndry/`
  captures508 unique chunks, peak summed Chromium RSS954,634,240 bytes and final-four
  spread9,031,680 bytes. Eviction limit still not exercised. The captured grass/forest
  view was inspected; relief improvement at every zoom/location is not established.
  This remains compressed20k candidate evidence, not canonical50k/global/hardware
  qualification. Read-only independent sampler review found no actionable defect.

## Reviewed species promotion (working tree based on 8243267)

Catalog v5 appends optional conifer/palm roles without changing prior role numbers,
resource kinds, IDs, amounts, passability, generator bytes or package identities.
Full raw prefixes 4654/9 and 4653/13 load strictly; only healthy conifer singles
[1,2,3,4,7,8] and palm singles [0,1,2,3,5,6,8,10,11,12] become wood presentations.
Native bodies use their own alpha silhouette, never broadleaf 2296 or unqualified
2304/2300. Empty optional families retain the exact healthy broadleaf fallback;
nonempty malformed groups are unavailable. Signed/outside anchors are preserved.
Culling and drawing share one body/shadow selection; body-only culling remains a
known limit for shadow-only edge visibility.

- `make GID=117 assets-verify`: pass for the unchanged original pack. Explicit
  Docker native private-manifest packing test: pass, all 678 actual extents
  (510 terrain/168 objects) on the existing three 2048² pages/50,331,648 bytes,
  with source/output bounds and overlap checks. Portable synthetic packing and
  an ignored private test alone are not this proof; missing explicit input fails.
- `make GID=117 fmt test-wasm perf-ci`: pass, actual 2 core/66 client/134 rendering
  WASM tests. All three backend pixel tests cover native selection/alpha/shadows;
  valid even-ID overlay snapshot/delta tests hide depleted native bodies and shadows.
  Lazy resource-then-unit submission is bit-for-bit compared with the previous
  materialized order across absent/present/malformed families and equal-depth ties.
- Initial gzip 228,194 exceeded the unchanged 228,102 cap. Compact index tables
  alone reached 228,187; an inlining annotation had no effect and was removed.
  Removing the unnecessary per-frame unsorted object-vector allocation retained
  exact submission semantics and reached 228,084 (18 bytes below cap). No baseline,
  profile, deadline, ABI, atlas, memory cap or test requirement was relaxed.
- Explicit original-art browser proof: pass on WebGPU, WebGL2 and Canvas2D with
  visible pixel removal and empty errors. Private captures were inspected: broadleaf,
  conifer and palm bodies fully visible. This uses synthetic flat fixture terrain,
  not a generated ecosystem or a hand-built production map. First capture failed
  visual qualification despite no JS errors: old source altitude focus and excessive
  wheel zoom projected bodies outside the viewport. The test now resets altitude
  focus after fixture binding and asserts visible pixel removal, not errors alone.
- Read-only independent source review found no actionable semantic defect.
  `make GID=117 browser-check fuzz-smoke hooks-install hooks-check`: pass.
  `make GID=117 fmt test-wasm preflight`: pass; 1,240 native tests/2 skips in
  233.187 seconds plus static/doctest/build/performance-smoke checks. One skip is
  the explicit private proof separately run above, not a packing success claim.
  The first attempt rejected stale source identity; an unchanged serialized retry
  caught a new fixture's banned unwrap. The fixture now explicitly asserts missing
  presentation failure, and the native helper lives in a proper test-only directory;
  no policy or production cache/protocol change was made.
- Pinned source qualification: pass with `make GID=117 test-country-source`,
  explicitly setting `AOE_SOURCE_QUAL_PACKAGE_DIRECTORY`,
  `AOE_SOURCE_QUAL_CONTENT_HASH` and `AOE_ASSET_PACK` to the unchanged inputs.
  An omitted package-directory invocation failed before running qualification;
  the first explicit attempt captured real Chromium `net::ERR_NETWORK_CHANGED`
  failures. An unchanged serialized retry passed, with all three backend error
  arrays empty. Local run-SBmaFD records 505 unique source chunks, peak summed
  Chromium RSS 952,934,400 bytes and final-four spread 4,055,040 bytes. Eviction
  qualification remains false; compressed candidate evidence is not canonical
  50k, global ecosystem, severe-zoom or hardware qualification. Commit/full E2E
  verification remain pending.

## Representation issues to address, not conceal

The candidate has real river evidence, but its 1024-axis hydrology grid spans
1200km: about1171.875m per cell versus modeled river buffer radii4..100m
(full corridor8..200m). A read-only code audit also confirms a separate registration
mismatch: preprocessing uses (i+0.5)L/N cell centers, but runtime hydrology uses
rounded endpoints x(N-1)/(W-1). At W20000/N1024/x10 it chooses1 instead of the
containing center-cell0; the first boundary is displaced about555m at60m/tile.
This arithmetic is not an executed fixture or attribution to a particular lake.
Any correction needs a new explicitly registered model/recipe and regenerated
identity while retaining old model1/2 readers; merely bumping the existing version
constant would reject model2. Do not alter the shared categorical helper in place.
Center-hit preprocessing loses narrow reaches; nearest-cell runtime lookup
expands retained cells into squares. Coarse128-axis inland fractions persist
beneath fine NoEvidence. Increasing cell occupancy or inventing a river path
would not fix source fidelity. Preserve vector reach topology and lake polygons
with holes in bounded, versioned, content-rooted source geometry; regenerate a
new opt-in candidate without reinterpreting existing package/model identities.
Modern inventories are not automatically circa600 observations; reservoirs and
regulated lakes retain their historical uncertainty. NODATA is not dry land.

Gray coastal bands also involve forced rock material on water-bank skirts.
Retain closed geometry, depth and picking while separating exposed rock from
sediment appearance. Beaches need generic modeled grade/exposure/shore policy,
not a French shoreline rule. Snow is blocked both by the generator's early
nonpassable return and the renderer's cliff-top Rock override. Snow-covered
summit tops versus exposed faces require explicit appearance semantics without
altering source heights or traversal. Regional density, minimap resolution and
resource readability need independent spatial/pixel and resource-budget tests.

## Visual references

Inspect the official [AoE II DE gameplay gallery](https://www.ageofempires.com/news/visual-look-at-aoe2de/)
for coherent soil patches, irregular grass/dirt transitions and sand/shallow/deep
water shore transitions. The [openage blendomatic documentation](https://simonsan.github.io/openage-webdocs/sphinx/doc/media/blendomatic.html)
describes adjacency-specific alpha masks and stable mask variation. The original
local blendomatic data is available outside Git; importing it requires bounded
parser tests and explicit asset provenance, not copying original bytes into Git.

Legacy defaults, source-lock identities, planner/start budgets, atlas/backing
limits, performance baselines and canonical512-chunk eviction gates stay intact.
Each stage requires focused Docker tests and full preflight before committing.
Start-region captures do not qualify rivers, coasts, alpine peaks or global travel.
