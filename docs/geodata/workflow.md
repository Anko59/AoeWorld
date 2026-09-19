# Geographic map workflow

The geographic map pipeline produces deterministic `MapPackage` inputs from
versioned, content-locked source data. It always keeps an explicit distinction
between source-backed map preparation and the procedural fallback used for
bounded tests and preview.

## Cache and source acquisition

The map commands run in the pinned Rust tools container. By default, source
data is cached at `.cache/geodata`; set `AOE_GEODATA_CACHE` to use a separate
absolute cache directory. The cache has a 100 GiB quota and each acquisition
job has its own byte budget. Keep source assets out of Git.

Run this on a connected machine before an offline generation job:

```sh
make geodata-bootstrap
```

It acquires the pinned global overview stack, verifies provider and SHA-256
integrity, and records the locks needed by map provenance. The stack currently
covers elevation, coastline, potential vegetation, and HYDE historical land
use. It does not silently substitute a downloaded file when a lock fails.

An offline machine can prove that its cache is ready without making network
requests:

```sh
AOE_GEODATA_CACHE=/data/aoeworld-geodata make geodata-verify
```

The command fails with the exact missing or corrupt source identifiers. Copy a
previously verified cache using a normal artifact-transfer process; do not add
the source downloads or generated cache to the repository.

## Request, estimate, generate, and verify

`AOE_MAP_REQUEST` names a JSON `MapRequest`. It contains the geographic
center, requested footprint, resolution and deterministic seed. The request is
normalized before any command uses it, so equivalent requests have a stable
identity.

```sh
AOE_MAP_REQUEST=/data/request.json make map-estimate
AOE_MAP_REQUEST=/data/request.json \
  AOE_MAP_PACKAGE=/data/maps/paris.json make map-generate
AOE_MAP_PACKAGE=/data/maps/paris.json make map-verify
```

The estimate command performs no acquisition. Generation requires the pinned
inputs and writes a bounded, atomically replaced JSON package. The package
binds the normalized request, local projection and preprocessing provenance,
source locks, and the complete prepared elevation, water, vegetation, and
historical-land-use page sets. `map-verify` rejects a tampered package, missing
pages, invalid environment roots, and source/provenance inconsistencies before
it reports the package content hash.

The local tangent-plane projection places the requested map center at `(0, 0)`
meters. Geographic raster and vector work happens in source-native
coordinates, followed by deterministic reprojection and reduction into the
bounded game pages. Terrain then derives water, cliffs and ramps, biome and
resource placement from those package inputs; the deterministic seed changes
detail placement without moving source-backed relief.

The map creator asks the native worker for a bounded, densified inverse-
projection of the effective square. It draws that boundary on its
equirectangular overview, including a split at the antimeridian. This preview
does not acquire sources or create a package.

## Tests and performance evidence

```sh
make map-test
AOE_MAP_REQUEST=/data/request.json make map-perf
```

`map-test` covers the deterministic map model. `map-perf` samples the bounded
procedural fallback at corners and center and labels its output
`synthetic_fallback_sampling`. It is useful for a quick regression signal but
does not demonstrate a source-backed RTS workload or dedicated-hardware
qualification. Use the repository performance gates for those claims.
