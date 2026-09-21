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
Before any transfer, bootstrap and source-backed generation report the complete
allowlisted source count plus cached and required download bytes; a batch over
the job or remaining cache budget fails before it begins.

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
  AOE_MAP_PACKAGE=/data/maps make map-generate
AOE_MAP_PACKAGE=/data/maps/<package-hash>.json make map-verify
```

The estimate command performs no acquisition. Generation requires the pinned
inputs and atomically publishes a bounded `<package-hash>.json` manifest in the
output directory. Page files live below `pages/<package-hash>/` with bounded
coordinate names for each layer and pyramid level. The manifest binds the
normalized request, local projection and preprocessing provenance, source
locks, and prepared page roots. `map-verify` streams those pages one at a time
and rejects tampering, missing or malformed expected pages, invalid coordinates,
and source/provenance inconsistencies before it reports the package content hash.

The local tangent-plane projection places the requested map center at `(0, 0)`
meters. Geographic raster and vector work happens in source-native
coordinates, followed by deterministic reprojection and reduction into the
bounded game pages. Terrain then derives water, cliffs and ramps, biome and
resource placement from those package inputs; the deterministic seed changes
detail placement without moving source-backed relief.

## Terrain, history, and compatibility limits

The current overview pipeline uses ETOPO elevation, Natural Earth coastline,
potential-biome data, and HYDE historical land-use inputs. It has versioned
locks and reproducible reductions, but it is an overview-quality stack. The
explicit [detailed preparation command](detailed-preparation.md) adds regional
Copernicus elevation while retaining coarse water and historical inputs.
ESA WorldCover, HydroLAKES, and HydroRIVERS remain unwired. Source availability, the
requested date, and the normalized package provenance are kept distinct so a
modern source cannot be mistaken for a historical reconstruction.

HYDE contributes a coarse historical land-use signal. It does not establish
the exact location of medieval forests, farms, settlements, roads, bridges,
or individual resources. Water and terrain rules use prepared pages and fixed
thresholds; resource placement is deterministic and suitability-based rather
than evidence for a particular real-world mine, herd, or stand of trees.

`MapPackage` is the game-facing protocol boundary. Its schema version,
normalized request, projection metadata, source locks, prepared-page roots,
generation recipe version, and content hash are validated before activation. The
server persists prepared pages alongside the manifest, serves immutable chunk
responses keyed by that hash, and rejects missing or inconsistent roots. Keep
package artifacts outside Git. When the generation recipe changes, retain the
old package directory for inspection and regenerate compatible packages from
the cached source inputs into the new versioned directory.

The local asset mapping is equally deliberate: the client maps semantic terrain
to six imported AoE II terrain groups (temperate grass, dry grass, dirt, sand,
rock, water) and four optional resource roles (berry bushes, broadleaf trees,
gold deposits, and stone deposits). Decorative natural features and other
unreviewed object sprites remain absent rather than being represented as
substitute art.

The map creator asks the native worker for a bounded, densified inverse-
projection of the effective square. It draws that boundary on its
equirectangular overview, including a split at the antimeridian. This preview
does not acquire sources or create a package. The same bounded worker request
compares fixed local-grid segments with WGS84 ellipsoidal distances and displays
the minimum and maximum projection scale error in parts per million. This
reports the selected extent's distortion; it does not describe continental
selections as distortion-free.

After a package has been generated, **Preview saved** requests a fixed 16 by 16
read-only terrain sample without activating gameplay. Its elevation, water,
reconstructed-biome, and passable-ground layers are useful technical
diagnostics, including for preview-only all-water or all-ice packages. The
passable-ground layer is not a spawn claim: the authoritative activation search
still decides whether a valid starting position exists.

The creation service runs one job at a time and queues at most two more. It
retains at most 128 job records in memory, evicting the oldest terminal record
when new work is accepted; running, cancelling, and queued jobs are preserved.
An evicted job ID is no longer available from job status endpoints. Saved map
packages remain independent of this history. Job history resets when the
server restarts. Preparation has no time estimate until measured progress is
available; a completed job reports zero remaining seconds.

## Tests and performance evidence

```sh
make map-test
AOE_MAP_REQUEST=/data/request.json make map-perf
```

`map-test` covers the deterministic map model. `map-perf` samples the bounded
procedural fallback at corners and center and labels its output
`synthetic_fallback_sampling`. It is useful for a quick regression signal but
does not demonstrate a source-backed RTS workload or dedicated-hardware
qualification. The supported evidence is therefore bounded deterministic
generation and synthetic sampling only; use the repository performance gates
plus recorded hardware, source-backed workloads, and a review of frame,
memory, worker, and network budgets before making a production performance
claim.
