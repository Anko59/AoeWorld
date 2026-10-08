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

## Representation issues to address, not conceal

The candidate has real river evidence, but its 1024-axis hydrology grid spans
1200km: about1171.875m per cell versus modeled river buffer radii4..100m.
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
