# River geometry and crossings

The creator defaults to Automatic rivers and landscape overview preparation.
Preflight includes the pinned HydroRIVERS Europe/Middle East and global
HydroLAKES inputs where the whole footprint is covered. Outside that coverage,
Automatic prepares the global overview without hydrology and records
`preparation_notes: ["outside_river_coverage"]` in the package and job result;
the creator displays the note. With rivers explicitly requires coverage and
rejects an uncovered footprint. Without rivers explicitly requests an overview.
Detailed regional elevation is an explicit choice and includes hydrology.

Without a geographic worker, only Automatic terrain preparation without an
explicit With rivers requirement permits procedural fallback. Its package and
job record `geographic_worker_unavailable`. Explicit source preparation and
With rivers requests reject the missing worker instead of changing their mode.

Coverage uses the exact geographic extent recorded from the SHA-pinned river
archive's shapefile header, inset by 0.01 degrees of query padding. The same
projected-footprint check runs before cache or input access on every hydrology
path. Its immutable decision is bound to the normalized request and grid and
reused by execution, including the CLI; sampling does not project it again for
coverage. The opened layer must match the pinned extent. This includes southern
Finland and replaces the former pilot restriction. File coverage does not prove
uniform completeness of the reach inventory.

Hydrology pages persist projected source line segments in 1/256 tile units.
The segment tuple is `[start_x, start_y, end_x, end_y, width, station_start,
station_end, source_delta_x, source_delta_y, source_station_delta]`. Original
direction and station span are retained before clipping and are required in the
single current format. Curve trims use the full source length and direction;
clipping cannot change the bend or its ford phase.
Lines are simplified by at most a quarter tile and clipped to their page plus
the maximum five-tile river half-width margin. Package validation rejects
segments outside that page margin through one contextual page validator used
by complete server verification, activation, provider decode/reload, generated-map
self-checks and streamed writer verification. It also checks exact page shape
and water-model presence against the index. This applies before lake/ocean or
correction shortcuts can skip physical river work.
At most 1024 compact ten-integer segments fit a page; existing page byte
limits also apply. Geometry, width, distance stations and original segment metadata participate in the page
content root. The required geometry field changes the single current format
in place: regenerate packages and clear chunk caches; there is no old reader.

For cells at most 60 geographic metres across, keep reaches with discharge
at least 0.1 m³/s. On coarser grids the cutoff is cell metres / 100, clamped
to 0.1–50 m³/s. Evidence cells intersecting the line corridor are retained
by exact segment-to-cell-rectangle distance at most the river half-width.
This data stage persists and validates geometry; terrain composition and river
presentation are separate changes. This stage adds no river terrain or rendering
behavior.

Explicit hydrology outside coverage fails with `vector hydrology footprint is
outside the pinned river dataset extent`. With a configured worker, Automatic
falls back only for this
preflight coverage outcome, never for acquisition or sampling failures. Source
errors fail the job. The single current package format requires the
`preparation_notes` array, even when empty; its note affects content identity
without changing the geography key. Regenerate packages; do not migrate them.

Regenerate only from verified cached inputs using `make map-generate` with
`AOE_MAP_HYDROLOGY_MODE=vectors`, then `make map-verify`. The geometry field is
required even on pages without reaches. Old local packages must be regenerated;
there is no compatibility reader. Real source completeness and dedicated
hardware performance require separate qualification.

## Terrain composition and minimap

Physical water uses distance to the source line, never the evidence cell's entire square. Bank-to-bank width is
`10 * sqrt(mean discharge)` metres, clamped to 5–600 metres, converted at the
map's geographic metres per tile and clamped to 3–10 tiles for playability.
Source bends survive sampling and tile/chunk boundaries. Runtime queries round
unambiguous connected vertices with quadratic cuts of at most six tiles or a
quarter segment, staying within three tiles of the source polyline. Page-clipped
ends and ambiguous junctions remain attached. A shared inverse world-coordinate
warp moves all intersecting reaches together: at most one and a half tiles per
axis on a thirty-two-tile noise lattice. A quarter-tile half-width variation is
clamped to the same three-to-ten-tile bank-to-bank rules. Neither shaping depends
on chunks, evidence cells or query order. Current output regression pins are regenerated when composition deliberately changes.

Fords span the whole drawn channel and five tiles of its length, repeating
every 40 tiles of distance along the source reach. A reach shorter than that
gets a centred crossing. These are gameplay crossings, not observed bridges
or historically documented fords. River and ford surfaces retain local land
heights so kilometre-cell water levels cannot create artificial cliff steps
at a crossing. Ordinary river water blocks movement; fords are emitted only where local slope checks pass; steep candidates stay
blocking river water. Artwork is handled by the renderer separately. Movement still checks edge heights. Resources, trees and dressing are
excluded from river water, including fords. The flat whole-map crossing
fixture flood-fills every passable tile from one bank through the fords.
This proves that fixture's connectivity, not unrestricted real-terrain routes.

Rivers grade from `Shore` at their edges to `Water` midstream, never deep.
Ford material and passability remain explicit. Land banks use textured dirt
in a noisy one-to-two-tile band, consumed by the existing shared ground mixer.
Passable river banks keep Dirt above the snow line; bank composition takes
precedence over snow cover as well as ground variety.
The renderer bevels shared convex and concave river-bank vertices by a quarter
tile at the finest LOD. Straight edges, diagonal connections, cliffs, incomplete
halos and unequal heights retain their exact corners. Both adjoining faces use
the same moved vertex; simulation heights and passability remain unchanged.
Fords use the required shallow-water sheet, including packs with optional depth
and marsh sheets. Browser pixel tests check Canvas, WebGL2 and WebGPU.
The minimap
uses the same persisted lines with pixel-half-diagonal footprint discovery, so a
sub-pixel river remains connected at strategic scale without widening its
physical channel. Lake and sea classes take priority at mouths. A composed
SetLand correction also wins at the raster sample; the raw footprint cannot
restore a river removed by that correction. Ordinary dry sample centres keep
footprint discovery for subpixel rivers.

Queries retain at most 64 page-local 16×16 segment indexes and 16,384 exact
physical water kinds. Tile keys are checked on memo hits; collisions miss.
The bins only filter candidates: integer channel distance and ford stations
still decide every hit. Provider rebinding clears both caches; failures are
never memoized and cancellation is checked even on hits. Lake/ocean observations and categorical coverage, lake/ocean models
and explicit land corrections skip channel work that cannot change their
result. Hydrology validation retains at most 64 weak immutable page identities,
rechecking a replaced or reloaded handle without retaining its payload.
Native page residency still retains at most 128 pages; searching its
LRU from the recent end preserves the same eviction order and avoids repeated
scans of old entries. Raster resolution, footprints and river visibility stay
unchanged.


Exact river composition stops promoting river evidence cells to categorical
lake coverage. Regenerate packages at this terrain stage so lake coverage
continues to identify lakes while source lines provide the river footprint.

Cliff cells reserve the widest river-bank halo as well as physical water. The
shared dense/provider check runs before terrain promotion and cliff decoration;
river and ford tiles and their textured dirt banks stay free of cliff cells.
Ground variety still skips bank tiles and continues on unreserved dry ground.
