# Detailed public DEM preparation

`aoe-map-worker map-generate-detailed` prepares a regional package from the
public, no-account Copernicus DEM COG distribution in AWS Open Data. Set
`AOE_MAP_DEM_RESOLUTION=glo90` for the worldwide GLO-90 layer, or `glo30` to
prefer GLO-30 Public and fall back to GLO-90 when a 1° tile is outside the
limited GLO-30 coverage. The worker resolves deterministic 1° tile names,
reads their public `HEAD` metadata, verifies the single-object S3 MD5 ETag
while downloading through the existing SHA-256 source cache, and records one
source lock per acquired tile.

For example, a 30 km request sampled on a 1024 by 1024 grid has approximately
29.3 m spacing:

```sh
AOE_MAP_REQUEST=/data/paris-request.json AOE_MAP_PACKAGE=/data/maps \
  AOE_MAP_SAMPLES=1024 AOE_MAP_DEM_RESOLUTION=glo30 make map-generate-detailed
AOE_MAP_PACKAGE=/data/maps/<package-hash>.json make map-verify
```

`AOE_MAP_SAMPLES` defaults to 128 and `AOE_MAP_DEM_RESOLUTION` to `glo90`.
Sample spacing describes the prepared grid; it cannot improve the native
source resolution. Copernicus is a modern surface model, including vegetation
and buildings. See the [public distribution](https://registry.opendata.aws/copernicus-dem/)
and [COG format notes](https://copernicus-dem-30m.s3.amazonaws.com/readme.html).

The first slice accepts 2 through 4096 samples per axis, at most 64 regional
tiles, at most 4 GiB of regional DEM input COGs, and at most 2 GiB of disk
staging. The retained overview stack has a separate 6 GiB per-job transfer
preflight and its existing cache quota; it is not charged against the regional
DEM acquisition limit. The regular `map-generate` entrypoint uses that same
overview preflight. It is a bounded preparation path, not the complete M2
world preparation: runtime page
residency, multi-region scheduling, and larger than 4096 sample requests are
outside this slice. COG samples are transformed from the request's local
azimuthal-equidistant grid and written as 64 by 64 pages. Pyramid reductions
are page-at-a-time from disk. Potential vegetation and HYDE pages are
nearest-resampled from the existing bounded 128-sample overview so the modern
DSM is not presented as new historical or land-cover detail. Water keeps the
overview values wherever the detailed source sampler has no supported class.
Hydrology samples currently cap at 1024 per axis; denser DEM grids nearest-map
those categories and do not imply finer water evidence.

Detailed water preparation also samples every WorldCover 2021 tile intersecting
the conservative request bounds, plus global HydroLAKES. The current HydroRIVERS
input is the Europe release and is sampled only when the full request bounds fit
inside the documented pilot window of 36–60°N, 12°W–25°E; outside it, river
evidence is explicitly marked unavailable in source-lock preprocessing metadata.
These are modern evidence sources; they do not rewrite HYDE's circa-600 land-use
estimates. Source locks record source metadata and SHA-256 digests.
HydroSHEDS archives use reviewed pinned hashes. WorldCover's fixed release
endpoint publishes no digest, so its narrowly allowlisted first HTTPS transfer
establishes a local SHA-256 pin; that first-use digest is not a provider-published
checksum. It is checked on every cache reuse. The HydroRIVERS input is the
Europe/Middle East release and is only evidence within the pilot bounds above;
outside them, the package does not claim river coverage. Coarse Natural Earth
coastline sampling and classified natural lakes/rivers may update the existing
prepared water layer. Wetlands, reservoirs, regulated lakes, and otherwise
unknown water remain evidence-only unless an explicit cited correction changes
the modeled cells.

Schema-9 packages persist `HydrologyEvidencePage` and `ModernLandCoverPage` as
separate level-zero evidence grids. Hydrology records the supported kind and
its acquisition method; modern land cover retains the raw WorldCover class.
Both grids participate in package roots, verification, and bounded residency,
and terrain reads them through the package provider. Legacy `WaterPage`
coverage remains present for schema-8 compatibility and overview fallback.
Detailed preparation adds a separately versioned water-model page to each
typed evidence page. It joins four-neighbor natural-lake cells across page
boundaries and assigns a common level from the lower quartile of adjacent
overview DEM heights. This is a coarse modeled level derived from ETOPO, not a
measured lake surface; the supporting elevation grid is capped at 128 samples
per axis even when hydrology detail is 1024. Lake tiles and uniquely adjacent
river-junction tiles expose that same level to terrain, rendering, picking, and
collision. An enclosed all-lake selection without shoreline samples falls back
to a consistent lower-quartile level from that component's overview DEM cells.
The junction value is a local tie only and does not imply a river direction or
longitudinal profile. HydroRIVERS v1 supplies reach IDs, downstream IDs, and
downstream distances, but the current reader retains only line geometry and its
discharge-based buffer width; it drops those topology fields before rasterizing
the buffered corridor. Flow therefore stays unknown and no downstream profile
is inferred. River cells without a lake or ocean junction use their resampled
overview DEM height as an explicitly modeled, directionless surface; this is
not a measured channel level. River membership is determined by center samples
at the declared hydrology-grid resolution. A cell classified as river is not a
claim that 100% of its sub-grid area is water; corridor width is represented at
that prepared resolution, with no finer bank or water fraction asserted. See
the [HydroRIVERS v1 technical documentation](https://data.hydrosheds.org/file/technical-documentation/HydroRIVERS_TechDoc_v10.pdf)
for the source topology fields that a directed follow-up must preserve.

`WaterCorrectionDocument` is an optional inline worker input for detailed
preparation. It binds patches to the normalized request, actual hydrology axis
(`min(detailed axis, 1024)`), local AEQD WGS84 projection, and target year 600.
It accepts at most 64 simple geographic polygons, each with a unique stable ID,
year interval containing 600, citation, operation, and precedence. Patches are
applied in ascending precedence then ID, so the final matching patch wins;
cell centers determine inclusion. `SetNaturalLake` and `SetLand` change only the
modeled field; raw modern evidence remains intact. The canonical document digest
participates in package identity. The document is limited to 24 KiB so it fits
in the 64 KiB worker request alongside optional historical corrections.
Omitted or empty corrections are valid; they apply no manual geographic
changes, while the documented natural-water model is still prepared.

Modeled-water packages use generation recipe 6; model-free overview packages
and older recipe-5 packages retain recipe-5 behavior and identity. Source-lock
preprocessing records `lake-surface-model-v1`. Modern lake/river extents remain
modern evidence used by the existing water policy; the model does not establish
that those extents existed in year 600. HYDE remains the source of the year-600
land-use layer, and modern WorldCover classes are not treated as year-600 land
cover. Reservoir, regulated-lake, wetland, and unknown observations remain
evidence-only unless an explicit cited water correction changes selected
cells.

Ordinary overview preparation now calls `prepare_hyde_area_600`. It extracts
the five allowlisted year-600 HYDE grids, checks that they share a grid, and
reads only each target page's intersecting source window. The area allocator
receives the complete source window for one target page of up to 64 by 64
cells. Crop area, grazing area, population, valid-land area, and coverage
totals remain unrounded until the historical pyramid is reduced. Detailed
preparation consumes this same area-allocated overview field; its larger
elevation grid does not imply finer historical source detail.

Source and target polygons are projected into a request-centered Lambert
azimuthal equal-area plane using deterministic 0.1-degree edge densification.
Each allocation call accepts up to 1,000,000 source cells and 4,194,304 input
polygon vertices; densified vertex totals use the same explicit bound. The
weighted historical-pyramid helper has its own 1,024-sample-per-axis cap,
independent of the overview and detailed elevation caps. Archive-backed ordinary
preparation currently remains at the 128-sample overview limit because it
retains the unrounded target allocation grid before pyramid reduction; the
1024-axis archive reader and reduction path must become page-incremental before
that production limit can be raised. Detailed elevation still uses the same
128-axis historical overview and does not imply finer history. HYDE lake
coverage now comes from polygon overlap between its 5-minute mask cells and
target cells, rather than assigning a full lake cell from a centre sample.
The weighted pyramid combines unrounded extensive quantities before writing
rounded land-use pages. Land cells missing crop, grazing, population, or the
valid-land denominator fail with a typed preparation error; numeric zero is
valid. Crop and grazing areas must fit within valid land area individually
and together. Target polygons touching or crossing a pole fail closed. The
allocator unwraps dateline polygons around the request center and preserves
their local geometry across split source windows. Lakes, ocean, nodata, and
uncovered area remain separate allocation totals. HYDE documents the mask's
declared nodata sentinel as ocean; unexpected non-finite mask values fail
instead. Target space outside the source raster is rejected rather than
published as source-derived zero. The current historical page schema does not
persist per-cell unknown or uncovered provenance; missing land quantities and
outside coverage fail preparation, and explicit unknown-page publication
remains follow-up work.

The public `prepare_hyde_600` function remains available for recipe-3 package
reproduction and keeps its nearest-cell semantics. Production source locks
identify the new preparation as `hyde-600ad-area-pages-v1`, so its package
identity changes through preprocessing identity and historical page roots.
Terrain generation semantics did not change, so the generation recipe remains
unchanged.

`HistoricalCorrectionDocument` is the versioned JSON contract for sparse
whole-cell corrections. Schema 1 targets 600 CE, accepts grids from 2 through
1,024 samples per axis, requires unique records in canonical row-major order,
and is limited to 64 MiB. Each tagged evidence value keeps HYDE's historical
model, a dated modern observation, a procedural addition, fallback data, or an
explicit unknown distinct; only HYDE evidence is exposed as historical
quantity. Complete whole-cell allocations become HYDE evidence, while nodata
or caller-uncovered area becomes explicit unknown evidence. Serialization and
deserialization validate the schema, target year, coordinates, quantity
capacity, ordering, evidence fields, and byte limit before accepting a
document; versions other than schema 1 fail closed.

The standalone allocation helpers keep `HydrologyPage` and
`ModernLandCoverPage` as preparation intermediates. Schema-9 publication
upgrades those inputs to the typed persisted evidence pages described above, so
supported lake/river kinds and raw WorldCover classes remain distinguishable to
terrain consumers. Separate modeled-water pages carry derived levels and
correction provenance without rewriting observations. Flow direction and
downstream profiles remain unknown where directed evidence is absent; no
barrier or confidence values are produced.
Hydrology planning is capped at 32 WorldCover tiles. A job may download at
most 2 GiB of missing hydrology sources; verified cache hits do not count
toward that transfer budget. Cache storage has its separate 100 GiB quota.
WorldCover reads are grouped into at most 2048 by 2048 pixel windows (4 Mi
pixels); each vector-sampled page is limited to 10,000 features and 16 MiB of
serialized transformed/retained geometry. River buffers count both their
transient source WKB and retained output WKB. This is an allocation guard for
the page's geometry data, not a hard process-RSS limit on GDAL internals.
Preparation currently stops through worker-process termination rather than a
live cancellation callback. A partial source download remains under its stable
cache key; the next attempt resumes from its verified byte offset with a
bounded ranged request, using 15-second connect/read/write timeouts.

The HYDE archive reader rejects target pages that touch or cross a pole before
calculating source windows. It unwraps target and source longitudes around the
request center and splits windows at a global raster's longitude seam, so a
dateline page reads only the cells on both sides of ±180°. The geocell detailed
preparation path still rejects footprints that touch or cross the antimeridian;
wrapped geocell acquisition is reserved for a later regional scheduling slice.
These bounds keep source-window sizes and coverage explicit.
An authoritative 404 for a catalogued Copernicus geocell supplies zero only
when the source-derived overview classifies the corresponding cell as 100%
ocean. Land, unknown, and partial-coast cells fail instead of being flattened
to zero; a coordinate outside the validated acquisition bounds or an
unselected geocell also fails.

The output uses the existing directory contract: a plain content-hash
manifest at `<output>/<hash>.json` and immutable pages below
`pages/<hash>/<layer>/`. The manifest is published last. `map-verify` streams
all expected pages and roots, so a failed or interrupted preparation cannot
be mistaken for a complete package. Generated source objects and package
artifacts belong in the ignored cache/output directories, never in Git.
