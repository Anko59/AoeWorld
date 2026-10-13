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
