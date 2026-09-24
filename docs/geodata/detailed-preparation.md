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
unknown water remain evidence-only until a historical reconstruction can
classify them.

The callable `prepare_hyde_600` path retains recipe-3 nearest-cell behavior for
existing packages. New recipe callers can use `allocate_hyde_area_window` and
`prepare_hyde_area_pyramid`: source and target polygons are projected into a
request-centered Lambert azimuthal equal-area plane using deterministic
0.1-degree edge densification, then crop area, grazing area, population, and
coverage area are allocated by polygon overlap divided by full source-cell
area. Each call accepts one target page (at most 64 by 64 cells), up to
1,000,000 source cells, and 4,194,304 input polygon vertices; densified vertex
totals use the same explicit bound. The
weighted pyramid accepts up to 1,024 samples per axis and combines unrounded
quantities and land areas before producing the existing rounded land-use
pages. Land cells with missing crop, grazing, or population values fail;
zero-valued land cells remain valid. Lakes, ocean, nodata, and uncovered area
are retained as separate allocation totals.

This API is the bounded allocation core. It accepts WGS84 source/target cell
polygons from its caller; it does not extract or read the HYDE archives. The
overview worker still uses `prepare_hyde_600`, so normal generation remains on
recipe 3 until archive-window enumeration, page assembly, and recipe identity
are wired by the generation orchestrator. Those preprocessing changes must be
included in the new recipe/package identity before it is used for published
maps.

`HydrologyPage` and `ModernLandCoverPage` are preparation intermediates, not
persisted package layers. Supported modern ocean/lake/river evidence is folded
into the existing `WaterPage` ocean and inland coverage percentages; lake and
river are therefore indistinguishable to the current terrain consumer. Flow,
barrier, confidence, and WorldCover class values are not retained in the
package. HYDE remains the source of the year-600 land-use layer, and modern
WorldCover classes are not treated as year-600 land cover. A typed persisted
hydrology layer and its terrain consumer remain follow-up work.
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

The first slice rejects footprints that touch or cross a pole or the antimeridian;
wrapped geocell selection is reserved for a later regional scheduling slice.
This fail-closed boundary keeps tile counts and source coverage explicit.
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
