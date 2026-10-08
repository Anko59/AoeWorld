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
