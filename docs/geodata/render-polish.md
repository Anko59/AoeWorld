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
surface. Simulation and geographic elevation remain unchanged.

## Terrain art and forest layout

Grass and dirt load all 100 flat frames from their original terrain sources.
World coordinates select the reviewed 10×10 sheet order: `x * 10 + (-y)` modulo
10 per axis. Partial ten-frame material groups retain two-axis variation.
Forest-floor ground uses dirt art instead of grass. The largest terrain LOD
still stretches one native tile over each coarse surface cell; transitions
between materials are not yet AoE-style blended borders.

Generation recipe 7 adds geography-keyed canopy density, irregular clearings of
varying size and spacing, and bent dirt trails between selected clearings.
Temperate ground follows the same masks: denser canopy gets forest floor and
openings get dry grass or dirt. A small irregular central glade preserves the
existing bounded start search without changing water, elevation, or slope rules.
These are procedural forest tracks, not historically sourced roads. Recipes
3–6 retain their generation behavior and remain loadable.

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
including hover inspection. World-layer ordering avoids constructing a second
full scene solely to remove sorting metadata.

## Validation scope

Run Dockerized `make test-wasm`, `make test-e2e`, `make browser-check` and
`make preflight`, with the repository hooks installed and checked before commit.
The client tests require all visible trees to survive dense viewport selection,
check the backing-resolution budget, and compare contact heights with coarse
projected triangle centroids. Existing browser tests exercise both WebGPU and
Canvas rendering, selection, camera movement and fallback startup.

`browser/tests/memory.shared-surfaces.spec.ts` checks a 4K CSS viewport at device
pixel ratio 2, then repeated dense-forest camera and resize cycles. It records
WASM linear-memory sizes and rejects continued growth after warm-up. This is an
allocation regression test, not evidence of full RTS performance, total process
RSS stability or qualification for thousands of moving units. Real-source art
must also be inspected locally at normal and minimum zoom; copyrighted pack
pixels and screenshots stay outside Git.
