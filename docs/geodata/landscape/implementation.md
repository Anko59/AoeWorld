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

## Open limits

Full-sheet seamlessness, expanded-art and shadow review, decoration drawing,
France capture/movement/memory, source-query cost, CI and dedicated-hardware
qualification remain open. Geodata acquisition still uses coupled DEM, water
and PNV axes; independent overview options and vectors-only hydrology are
pending.
