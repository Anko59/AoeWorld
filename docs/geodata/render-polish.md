# Geographic map rendering polish

This pass addresses the playable France map: visible tile seams, disappearing
canopy at distant zoom, missing shadows, monotonous ground, regular clearings,
unit contact on simplified terrain, and browser memory pressure.

## Visibility and terrain contact

Resources are culled against their actual projected sprite bounds. The former
1,024-resource screen-grid sampling is removed: zooming out must not delete
visible trees. The decoded chunk cache remains limited to 512 chunks / 128 MiB,
and the WebGPU instance buffer retains its 64 MiB ceiling.

The client caches one projected terrain scene for the current camera and
resident terrain. Chunk insertion, eviction and reset invalidate it. Rendering,
mouse picking and object ground contact use this same mesh. A spatial index
samples the displayed triangle height for units and resources, including coarse
terrain LOD, so fine source heights cannot put their feet beneath the displayed
surface. Material vertices and contact buckets share a compact coordinate index:
each key is stored once, while separate values retain original first-hit triangle
order. Keys are bounded by the displayed mesh, not the full resident world.
Geometric vector growth can temporarily exceed an ordered map's storage at some
counts; the browser residency tests cover the combined footprint. Simulation
and geographic elevation remain unchanged.

## Terrain art and forest layout

All six terrain materials load all 100 flat frames from their original sources.
Atlas packing sorts physical placements by height while retaining role/frame indices.
World coordinates select the reviewed 10×10 sheet order: `x * 10 + (-y)` modulo
10 per axis. Partial ten-frame material groups retain two-axis variation.
Forest-floor ground mixes ten optional native Forest/g_for 15011 accents with
periodic dirt. Missing forest art explicitly retains dirt; all six existing
terrain groups retain their 100-frame grids and the single 2048² atlas. Coarse height cells are
split into bounded texture patches, preserving their original surface planes
and contact heights while reducing stretched grass diamonds at distant zoom.
World-keyed vertex materials crossfade adjacent grass/dirt/sand/rock art on
both WebGPU and Canvas. Water and discontinuous cliffs are excluded. This is
procedural texture splatting, not the original game's authored transition masks;
it can soften very narrow dirt tracks. The unchanged 24,576-triangle budget still limits texture detail at extreme
viewport sizes; these views may scale a native tile over multiple source tiles.

Generation recipe 7 adds geography-keyed canopy density, irregular clearings of
varying size and spacing, and bent dirt trails between selected clearings.
Temperate ground follows the same masks: denser canopy gets forest floor and
openings get dry grass or dirt. A small irregular central glade preserves the
existing bounded start search without changing water, elevation, or slope rules.
These are procedural forest tracks, not historically sourced roads. Recipes
3–7 retain their generation behavior and remain loadable. Recipe 8 adds small
opening nodes in formerly empty cells, mandatory bent cardinal tracks, and a
narrow connection from the central glade to its cell's node. These masks clear
procedural resources, never water or cliffs. Recipe-8 accepted starts must also
certify an actual terrain/resource-crossable route at least 64 tiles away on
large worlds. Small worlds use `min(64, max(1, (max(width, height)-1)/2))`
tiles so an edge-directed certificate is possible (31 on a 64-tile map, still
beyond the central glade's 27). The unchanged bounds are 4,096 visited exit
tiles and 64 exit chunks across the start search.
Cancellation or an exhausted bound is not proof of absence. Natural barriers
can split the procedural graph; unqualified starts are rejected, not teleported.
Existing recipe-7 packages require regeneration into a new content-addressed
recipe-8 package to use these tracks; their saved identities are not upgraded in
place. Source locks and prepared page roots remain applicable when geodata is
unchanged.

Broadleaf trees use their matching original shadow frames and a frame selection
that favors leafy crowns while retaining some bare trees. Other resources and
the horse use a flattened alpha silhouette with a fixed sun direction. Canvas
and WebGPU both apply shadow tint and opacity. No original assets are committed.

## Browser memory

The canvas backing store is capped at 4,194,304 pixels and 4,096 pixels on either
axis. Pointer coordinates use the actual backing-to-CSS ratio. This bounds the
Canvas raster color/depth buffers to 48 MiB combined and each RGBA presentation
surface to 16 MiB. These are individual allocation bounds, not a total browser
RSS guarantee. Atlas, mesh, chunk, browser and GPU allocations are additional.

A projected mesh is reused while the camera and loaded terrain are unchanged,
including hover inspection. Unchanged static frames reuse the last successful
presentation; selection, movement, resources, grid and terrain changes redraw it.
World-layer ordering avoids constructing a second full scene solely to remove
sorting metadata. Object stable IDs travel into the final painter sort,
eliminating the redundant object sort without changing depth/kind or
shadow/body tie ordering. Fixed selection/debug rings use bit-exact precomputed
WASM angles, while wheel-only exponential zoom uses browser math.

The loader preserves required manifest types and fields but does not retain
unused metadata strings. Selection and packing use indices instead of cloned
frame records; raw manifest bytes are released before PNG decoding. A small
startup-only insertion sorter handles the reviewed catalogue (at most 746
selected frames); invalid source counts are rejected before sorting. It is not
used for unbounded or per-frame painter ordering.

## Validation scope

Run Dockerized `make test-wasm`, `make test-e2e`, `make browser-check` and
`make preflight`, with the repository hooks installed and checked before commit.
The client tests require all visible trees to survive dense viewport selection,
check the backing-resolution budget, and compare contact heights with coarse
projected triangle centroids. Existing browser tests exercise both WebGPU and
Canvas rendering, selection, camera movement and fallback startup.

`browser/tests/memory.shared-surfaces.spec.ts` checks a 4K CSS viewport at device
pixel ratio 2, then six dense-forest camera and resize cycles. It records WASM
linear-memory sizes and rejects more than 16 MiB growth after warm-up. It also
samples all Chromium processes through CDP and Linux `/proc`: summed VmRSS must
stay within 1 GiB, with at most 64 MiB variation over the final four cycles.
The maximum-backing sample and every cycle's browser, renderer, GPU and utility
process sizes are preserved as test evidence, including on assertion failure.
Shared pages are counted repeatedly, so this is a conservative residency sum,
not private-memory measurement. It excludes host GPU allocations, is not a
long-route/source-map soak, and does not qualify thousands of moving units.
`make test-memory-source` adds an opt-in real-pack source-map route. Supply
`AOE_POLISH_SOURCE_URL` for an already running server with the saved map active,
and its `AOE_POLISH_SOURCE_HASH`. It does not reset or change the world: 24 long
middle-button camera drags visit more than 512 unique source chunks and return
to the primary unit. WebGPU, forced Canvas and default Chromium each require
resident chunks within 512, summed sampled VmRSS within 1 GiB, final-four-sample
variation within 64 MiB and no browser exceptions. Numeric evidence is retained
in the browser test output directory. Without both inputs this external check
is skipped, not qualified by ordinary E2E. It still has one stationary unit,
not a moving-army workload or indefinite soak. Real-source art must also be
inspected locally at normal and minimum zoom; copyrighted pack pixels and
screenshots stay outside Git.
