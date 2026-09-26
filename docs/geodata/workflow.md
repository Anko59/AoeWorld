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

Runtime activation uses the same verified page roots through a bounded provider.
It retains at most a 64 MiB compact page index per package and 128 decoded
environment pages per provider, with deterministic least-recently-used
eviction. The server keeps at most two indexed providers in its registry;
active gameplay retains its provider handle until that world is released.
Indexing and page reads are cancellation-aware and request-local. A missing,
corrupt, or cancelled page request fails the preview or chunk response instead
of returning a procedural substitute or a partial chunk. These are residency
and request bounds, not source-backed RTS performance qualification.

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
Copernicus elevation and samples modern WorldCover, HydroLAKES, and the bounded
Europe/Middle East HydroRIVERS pilot while retaining the overview's coarse
vegetation and historical inputs. Schema-9 packages persist typed hydrology and
modern land-cover evidence in bounded, independently verified pages. Terrain
consumes those observations through the package provider while retaining the
legacy water coverage layer for compatibility; schema-8 packages remain
readable with typed evidence absent. Source availability, the requested date,
and normalized package provenance remain distinct so a modern source cannot be
mistaken for a historical reconstruction.

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

`AOE_GEODATA_CACHE=/absolute/cache make test-creator-source` runs the ordinary
Paris overview creator in Chromium, checks the visible footprint and estimate,
measured page progress, source-backed activation and layer preview, then restarts
the server without a worker or source cache. The second browser session opens
the same saved map and requests a previously unseen chunk from published pages.
The revision-bound result, source locks, package identity, and captures are
written under ignored `reports/creator/`. This journey uses the fixed Paris
request in `reference-matrix.json`.

`AOE_GEODATA_CACHE=/absolute/cache make test-geographic-matrix` prepares the
fixed 600 CE overview requests in `reference-matrix.json` with the ordinary
worker. It verifies each published directory. The
runner records each case and any typed failure in ignored
`reports/geodata/matrix.json`. It does not attempt starting-position activation,
so an uninhabitable selection can still pass generation. Matrix generation alone does not establish activation,
visual fidelity, or memory qualification for all regions.

After matrix generation, `make test-geographic-visuals` requires source-backed
Southern Finland lake, Nile Delta coast, and Central Alps packages. It verifies
nonzero inland/ocean page coverage and at least 1,500 m of Alps relief before
launching the ordinary client on both WebGPU and Canvas. Captures and metadata
are written under ignored `reports/geographic-visuals/`; metadata binds each
image to its package hash, request, generation recipe, source locks, asset
manifest identity, backend, loaded chunks, and camera. The small fixed maps
contain fewer than the 512 chunks needed to exercise eviction. Chromium uses
SwiftShader, so the capture does not qualify dedicated GPU performance. The
runner records each case's page count, page bytes, and chunk-grid upper bound;
it rejects packages above 64 pages, 1 MiB, or 512 chunks. It samples the server's
resident memory across the full browser run and rejects peaks above 1 GiB; that
aggregate does not isolate per-case server memory or the browser container.
`AOE_MAP_PACKAGE_DIRECTORY` can point the server at a verified package
directory without copying packages into the normal creator store.
Historical land-use metadata counts known coverage samples separately from
legacy pages that omit coverage; omitted values are not treated as land or zero.

The original `coast_estuary` request was centered on Cairo and produced zero
water coverage, so it was rejected as a coast capture. The corrected request
uses 31.4 N, 31.5 E; `coast-correction.json` records both package hashes,
requests, water-page evidence, and source-lock identities. The corrected package
has nonzero ocean coverage but no inland-water samples, so this capture
qualifies a Nile Delta coast case (renamed `nile_delta_coast`) and does not show
an estuary channel. The `river_lake` case remains the separate inland-water
qualification.

The creation service runs one job at a time and queues at most two more. It
retains at most 128 job records in memory, evicting the oldest terminal record
when new work is accepted; running, cancelling, and queued jobs are preserved.
An evicted job ID is no longer available from job status endpoints. Saved map
packages remain independent of this history. With a package directory, bounded history is atomically checkpointed under
`jobs/history.json` before accepting a request, cancellation, or completion.
A restart restores terminal records and monotonic IDs. Interrupted queued or
running jobs become failed with an explicit retry message; cancellation requests
become cancelled. The creator lists retained requests and can retry failed/cancelled work using
its original coordinates, scale, seed and preparation mode. Accepted job IDs
remain in browser storage across reloads and status/activation network errors;
Resume checks the existing job instead of submitting another preparation.
Running or completed work can also be resumed through the history selector. Completed
history requires the matching stored package; a missing package becomes a
retryable failure. The worker does not automatically resume execution after a
restart, but verified source cache entries are reusable. Without a package
directory history remains memory-only. Malformed or oversized history fails
startup explicitly instead of silently dropping accepted work. Preparation has no time estimate until measured progress is
available; a completed job reports zero remaining seconds.
Completion is published only after the map is registered for package retrieval
and activation. Cancellation received while registration is waiting wins over
success; the cancelled result is not registered by that job. Completion and queue advancement share one history checkpoint before becoming
observable. A failed checkpoint rejects new requests/cancellation without
mutating the manager; failure to record a worker outcome marks it and queued requests failed,
retaining their original requests for retry rather than leaving a stalled queue,
and does not publish its package in the running registry. Complete immutable
package files may already exist; startup discovers these valid saved packages
independently of failed/cancelled history, without activating gameplay. The journal is limited to 2 MiB and 128 jobs;
one fixed staging file bounds crash leftovers. One process owns the directory.
Directory-sync failure after rename reports uncertain crash durability in the
server log while retaining the committed state. Partial geographic preparation
files are not garbage-collected by this journal.

The creator's Automatic detail mode selects regional Copernicus elevation for
squares up to 120 km with centers within 75 degrees latitude and 170 degrees
longitude. This conservative window avoids the regional worker's unsupported
polar and wrapped footprints. The grid is a power of two between 128 and 4096
samples, targeting 30 m spacing. Larger or edge-of-world selections explicitly
use the 128-sample overview; a user may also select overview for a small map.
Detailed mode rejects selections outside that window instead of silently
downgrading them. Exact projected coverage and source/staging budgets are still
checked by the worker before preparation. Water, vegetation and historical
land use retain their current overview resolution, even with detailed elevation.

Estimates and active jobs expose the selected preparation mode, sample grid,
spacing, and source limitations. Without a configured worker, Automatic remains
explicitly procedural fallback; an explicit source mode returns an error.
Selecting detailed mode never turns a source failure into procedural terrain.
These grid spacings describe sampling, not a guarantee of source accuracy or
historical precision. Source transfer size and completion time are not yet
estimated; the existing source quotas apply.

## Mutable resource snapshots

`ResourceOverlaySnapshot` schema 1 binds depletion state to the complete map
content hash. Changes are sorted by stable resource ID and contain only amounts
below the generated initial amount. Loading validates the schema, map identity,
IDs, amounts, ordering, revision, and availability for referenced resources
before returning a
replacement overlay. Failure leaves the caller's current overlay untouched.
The revision counts changing depletion calls; zero-removal calls do not advance
it. Restored collision and subsequent depletion use the same authoritative state.

At most 65,536 changed resources may be retained or decoded in one overlay.
Adding another changed resource fails explicitly without partial depletion;
already changed resources can still deplete. This is an entry count, not an RSS
claim. The map crate owns the snapshot contract; the server owns persistence and
network publication, and the client keeps mutable amounts outside chunk storage.

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

Resource persistence belongs to the authoritative server process. Activation
loads `resource-overlays/<content-hash>.json` before choosing a start or spawning
units. Files are limited to 4 MiB, and snapshot entry limits still apply.
`GameplayService::deplete_resource_persisted` validates an exclusive candidate,
writes and syncs a temporary file, replaces the snapshot, then commits live
collision state. A rejected write leaves the world unchanged; a failed directory
sync after replacement commits live state and explicitly reports uncertain
crash durability. A stale service revision cannot overwrite newer saved state.
A fixed temporary name limits crash leftovers to one per map and the next
write recovers it under the process lock. One server process must own a package directory; this is not a shared database.
No gather command or economy is introduced.

Gameplay protocol 7 synchronizes sparse resource amounts. Every subscription
receives a complete reset, including an empty reset for an unchanged map. Later
messages name the exact previous overlay revision and contain sorted final
amounts for changed IDs. The server retains 1,024 revision entries, coalesces
repeated IDs, and sends a fresh bounded snapshot when a client has fallen behind
that journal. Both forms carry the subscription revision; superseded replies
cannot change the current view. An unsuccessful resource queue write disconnects
the consumer rather than advancing its acknowledged publication state.

The client retains at most 65,536 changed amounts separately from immutable
chunks. Exhausted resources are filtered before rendering, so chunk eviction and
reload cannot restore their sprites. Unknown resource state hides sprites until
the initial reset arrives. Welcome and world replacement clear this state;
a malformed or discontinuous update closes the socket and requires reconnect.
Reset/delta validation, persistence-to-wire reconnect, queue overflow, and cache
capacity are covered by native tests; both browser render backends are covered
by the general E2E suite. Those tests do not yet constitute a source-backed
interactive depletion or long-session performance qualification.

Worker lifetime is bounded in the Dockerized Linux server. Preparation has a
two-hour deadline and footprint projection a 30-second deadline. Cancellation,
output overflow, deadline expiry, and normal worker exit terminate the worker's
owned process group, including GDAL descendants, before joining pipe readers.
Requests remain limited to 64 KiB, responses to 512 KiB, and diagnostics to
8 KiB. Oversized output fails explicitly; readers drain without retaining excess
bytes until termination, so a full pipe cannot prevent cancellation. Input is
written separately so a worker that never reads stdin remains cancellable.
The durable job journal described above records interrupted work; process
termination alone does not remove every partially prepared source/cache file.

Server-driven detailed preparation owns a separate directory below
`<cache>/worker-scratch/`. The server and worker hold shared file leases;
startup and the next preparation recover only directories with no live lease.
Normal completion, failure, and cancellation remove the owned scratch after the
worker group has been reaped. This bounds crash leftovers without deleting an
active worker's pages. Cached source objects and resumable downloads remain
shared. This ownership applies to detailed pyramid staging, not to every
provider extraction temporary or incomplete immutable publication file.
Direct CLI preparations retain their existing local staging cleanup behavior.

Preparation status has optional version-1 `progress` evidence. Named phases
cover source acquisition, overview/water sampling, pyramid construction,
publication, and server verification. Downloads report bytes for the current
source only; detailed pyramid construction reports persisted pages against
its exact four-layer total, including partial edge pages and all levels.
Counts are phase-local, never an overall completion percentage or time estimate.
Unknown totals remain absent. Active status exposes these counters only while
running; cancellation and terminal history do not retain stale progress.

The worker atomically replaces a small progress file in its leased scratch,
at most four times per second except phase changes and final counts. The server
polls at most five times per second and accepts only regular files, version 1,
known phases/units, at most 4 KiB, and coherent bounded counts. Malformed or
missing progress retains the last valid observation and cannot publish a map.
Progress is best-effort telemetry: write failures do not change job outcome,
and brief phases may pass between polls. The existing cancellation, process
and output bounds remain authoritative. No overall percentage is supplied until
successful completion; no remaining-time estimate is manufactured.

Creation POST requests may carry `Idempotency-Key`, exactly 32 lowercase hex
characters generated independently of the map seed. While its job remains in
the bounded 128-record history, the same key and normalized request/preference
return the original job, including after restart, without another worker launch.
Changing coordinates, seed, scale or preparation preference with an existing key
returns HTTP 409. Invalid keys return 400. Journal schema 2 persists keys and
continues to read schema 1; malformed or duplicate saved keys fail startup.
Retiring a terminal history record also retires its key: this is bounded request
recovery, not an indefinite global deduplication service.

The creator stores the key and original request before POST, then replaces them
with the accepted job ID in one localStorage value. After a lost acknowledgement,
`Recover request` resends that exact submission after reconnecting. Transport or
server failures keep the key; explicit 400/409/429 rejections permit a corrected new
request. A returned job ID resumes normal polling and activation. If browser
storage is unavailable, same-page retry still reuses the key; reload recovery
then relies on server history. Deliberately retrying a failed/cancelled job uses
a new key so it can perform new work.
