# River geometry and crossings

The creator requests vector hydrology by default. Overview preparation adds
the pinned HydroRIVERS Europe/Middle East and global HydroLAKES archives to
the seven overview locks; detailed preparation already requests hydrology.
Coverage uses the exact geographic extent recorded from the SHA-pinned river
archive's shapefile header, with the 0.01-degree query padding removed. Preflight
checks the projected footprint before acquisition; sampling checks the opened
layer against that metadata and uses the same footprint validator. This removes the former western-Europe pilot box, including its
exclusion of southern Finland. An extent check establishes the available
regional file, not uniform completeness of its reach inventory.

Hydrology pages persist projected source line segments in 1/256 tile units.
Lines are simplified by at most a quarter tile and clipped to their page plus
the maximum five-tile river half-width margin. Package validation rejects
segments outside that page margin through one contextual page validator used
by complete server verification, activation, provider decode/reload, generated-map
self-checks and streamed writer verification. It also checks exact page shape
and water-model presence against the index. This applies before lake/ocean or
correction shortcuts can skip physical river work.
At most 1024 compact seven-integer segments fit a page; existing page byte
limits also apply. Geometry, width and distance stations participate in the page
content root. The required geometry field changes the single current format
in place: regenerate packages and clear chunk caches; there is no old reader.

For cells at most 60 geographic metres across, keep reaches with discharge
at least 0.1 m³/s. On coarser grids the cutoff is cell metres / 100, clamped
to 0.1–50 m³/s. Evidence cells intersecting the line corridor are retained
by exact segment-to-cell-rectangle distance at most the river half-width.
This data stage persists and validates geometry; terrain composition and river
presentation are separate changes. This stage adds no river terrain or rendering
behavior.

Every hydrology preparation route (vectors, full, detailed and overview vectors)
uses the same pinned-extent footprint validator before any cache or input access.
Requests outside coverage fail with `vector hydrology footprint is outside the
pinned river dataset extent`; there is no silent no-river fallback. Overview
preparation explicitly requesting no hydrology remains available worldwide.
The opened dataset extent must equal the pinned metadata before sampling.

Regenerate only from verified cached inputs using `make map-generate` with
`AOE_MAP_HYDROLOGY_MODE=vectors`, then `make map-verify`. The geometry field is
required even on pages without reaches. Old local packages must be regenerated;
there is no compatibility reader. Real source completeness and dedicated
hardware performance require separate qualification.
