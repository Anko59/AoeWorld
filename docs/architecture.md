# Architecture

The local Tokio server owns two deliberately separate paths. The gameplay
service at `/game/ws` advances one fixed-point `GameWorld` at 20 Hz, stores only
occupied 32×32 chunks, and sends bounded snapshots and tick deltas for each
client's tile subscription. The gameplay protocol is independently versioned
from the diagnostic protocol. A match hosts several seats ([AI SDK](ai-sdk.md)):
each slot owns one player and is reached through its own opaque token (a plain
`Hello` resumes with a token or claims the first free human slot, `JoinSlot`
authenticates for exactly one slot), and each controller receives only its
player's fog-filtered view. Other connections spectate the first slot, and a
disconnected human controller has a 30-second lease before the oldest spectator
is promoted. Move orders are
canonicalized to the center of a valid tile; fixed-point interpolation carries
units continuously between those lattice waypoints.

Gameplay starts with an owned Town Center, three villagers and the scout.
The simulation owns integer stockpiles, population, gathering jobs and a serial
villager training queue. Building footprints block authoritative navigation.
Gathering uses the resource overlay transaction and owned Town Center drop-off
points. Snapshot and tick messages carry bounded player/building state alongside
unit kinds, tasks and carried loads; failed outgoing delivery closes the session.
The single current rules table is described in [village economy](economy.md).

The `/diagnostics.html` adapter retains the synthetic compatibility world and
wire format for regression evidence. Its world is initialized only when a
diagnostic endpoint is used, so opening the game does not start the synthetic
population. Diagnostic camera movement and zoom still change its bounded
subscription and its timing counters remain separate from gameplay health.

The main `/` page is a presentation client. It receives authoritative
fixed-point positions, keeps a bounded two-tick interpolation history, and
contains only camera, selection, connection, and rendering state. Shared Rust
projection math provides the 2:1 isometric camera and inverse picking. Rendering
prefers WebGPU and falls back to Canvas 2D; both paths use native-scale local
AoE II sprite frames, viewport culling, and semantic temperate, dry, dirt,
sand, rock, and water terrain groups from immutable map chunks. The server's
environment reads are isolated in
`crates/server/src/config.rs`.

Before fetching geographic terrain, the client requests the package's verified
minimum and maximum game heights. The server scans source elevation and modeled
water pages through bounded residency off the async runtime, then caches the
result with that immutable package. Bilinear interpolation stays inside those
source bounds. The client projects the resulting height interval into a narrow
viewport strip instead of exploring the world square. Compact height probes
let it discard off-screen candidates without repeatedly downloading them, and
visible relief takes priority during eviction. Decoded terrain remains capped
at 512 chunks and 128 MiB, with at most 64 concurrent requests. Height metadata
and candidate coordinates each have a separate 65,536-entry ceiling; very large
views remain bounded rather than allocating in proportion to map area.

Terrain composition shares two off-tick workers with navigation warmups. A
connected human's authenticated camera requests use a priority queue with 64
reserved admissions; background and AI work have a separate 64-request bound.
Running jobs finish before the next job is chosen from the human queue first.
Match startup retains rendered chunks around the map-centre camera and human
starts in a bounded cache, so HTTP can reuse that work. Complete static
navigation disables redundant speculative walk-chunk preparation. Terrain
timing logs separate time in the queue from time spent executing the work.

The minimap and whole-map overview draw one strategic raster per package from
`GET /maps/{hash}/minimap`. The server samples the composed landscape query
that also produces chunks, at one sample per tile up to 512 per axis, so the
raster follows the world. Persisted river lines are additionally tested against
each pixel footprint so narrow rivers stay visible at strategic scale; physical
channels retain their tile-scale widths. The body is `AWMM`, a little-endian
axis, two zero bytes and one byte per sample: terrain class (grass, dry grass,
forest, soil, sand, rock, snow, ice, shore, river, lake, ocean) in the low
nibble and north-west relief shade in the high nibble, at most 262,152 bytes.
One raster is generated at a time off the async runtime within 30 seconds,
never partially, and the four most recent are cached. The client rejects any
other shape and keeps its schematic map; it requests no chunks for the map.
Relief averages the unquantized directional slope with a mirrored separable
box before encoding its shade. Its radius follows four elevation facets,
bounded by a four-pixel minimum and one eighth of the raster axis (smaller
rasters use a smaller window). Sliding sums keep filtering linear in raster
area, with bounded scratch; class queries, colours and water shading are unchanged.

Native map preparation streams immutable navigation on at most four workers,
with at most eight encoded chunks queued for its writer. It never collects a
whole-map chunk vector. The current format (2) stores walk runs, 16-byte resource
records, component-label runs, 8-byte portals and flat u32 portal costs (the
maximum u32 means unreachable). Local coordinates imply global tile positions.
Every tile, resource identity, directed crossing and exact cost is preserved.

The server atomically publishes a checksummed binary snapshot under
`map-packages/navigation/<package-hash>-<navigation-format>.bin`, using the
existing package lock. Staging inodes are private; published inodes are read-only
and must never be edited or truncated. Regeneration replaces the pathname.
Native adapters use memmap2's safe mapping API, validate all records before
installation, and retain only an offset index on the heap. Kernel-immutable
fs-verity files map directly. Ordinary cache files stream through a 1 MiB
buffer into a sealed Linux memfd before mapping; no first-party unchecked map
or heap-sized file copy is used. Walk, component,
portal and cost queries then read the mapped bytes directly; no static terrain,
graph or cost construction occurs on the tick thread. Overlay changes still
rebuild affected local graphs from the immutable walk data under planner budgets.
The pure map crate owns byte views and validation, with no filesystem or OS API.

Files are limited to 2 GiB and retained index entries to 16 MiB (4 MB of entries
for France's 250,000 chunks). Validation temporarily duplicates the bounded index
to reject overlaps, gaps and trailing data. Four walk caches are each capped at
64 MiB; encoded messages are individually capped at 4 MiB. These are logical
bounds, not a measurement of total RSS. Source residency, simulation and sessions
have separate budgets. Larger indexes remain lazy without allocating in
proportion to map area. Format changes regenerate derived data; no legacy
reader is retained. Files stay outside Git and container images.

Package creation bakes off tick before reporting readiness. A match maps an
existing file off tick; when it is missing or another bake is running, the
match continues with lazy navigation and registers a background installation.
One background bake runs per process with at most eight package requests and
256 weak installation targets per package. Retired worlds cancel their requests
without pinning caches. Concurrent package requests use the existing native lock.

Verified snapshots share one Arc across preparation and match replacement;
fs-verity file pages are shared and evictable across processes. The sealed
memfd path shares one compact copy within the process; its pages require swap
for eviction, and separate server processes retain separate sealed copies. The snapshot LRU
retains at most 1 GiB of logical mapped bytes and 256 handles, with weak handles
for live worlds after eviction. Health snapshot bytes include the mapped file
and index; they are not resident heap or RSS. Mutable caches remain world-local.
See [large-map navigation measurements](scale/static-navigation.md) for the
synthetic before/after workload and its qualification limits.
Resource-candidate blocks have an 8,192-entry, 32 MiB LRU, including empty
blocks. Walk and dynamic portal caches each retain at most 64 MiB.
`/health` exposes live world/session counts and retained snapshot, atlas and
cache capacities; snapshot loading also logs its allocation progress. These
logical capacities exclude allocator slack and do not replace RSS measurements.

`aoe-map` is the environment-independent boundary for frozen geographic map
identity. It validates the 600 CE request, physical compression and bounded
virtual dimensions, canonicalizes equivalent ratios and longitudes, and
generates deterministic 32×32 terrain/resource chunks from package inputs.
It deliberately has no filesystem, networking, browser, GDAL, or PROJ
dependency. The configured native preparation worker owns those capabilities,
executes source-backed preparation out of process, and has a separate bounded
request for projected creator-map footprints.

Mutable resource amounts are separate from immutable geographic chunks. The
server restores map-bound snapshots before spawning and commits depletion only
after persistence accepts it. Gameplay protocol 2 sends bounded sparse resets
and exact-revision deltas; lagging consumers reset from the authoritative overlay.
The client retains that state independently of chunk residency and removes
exhausted sprites on both render backends. This supplies resource lifecycle
infrastructure; gathering commands and an economy are outside this change.
