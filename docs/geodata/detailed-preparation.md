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
are page-at-a-time from disk; water, potential vegetation, and HYDE pages are
nearest-resampled from the existing bounded 128-sample overview so the modern
DSM is not presented as new historical or land-cover detail.

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
