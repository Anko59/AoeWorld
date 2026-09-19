# Architecture

The local Tokio server owns two deliberately separate paths. The gameplay
service at `/game/ws` advances one fixed-point `GameWorld` at 20 Hz, stores only
occupied 32×32 chunks, and sends bounded snapshots and tick deltas for each
client's tile subscription. The gameplay protocol is independently versioned
from the diagnostic protocol. The first controller receives an opaque resume
token; later connections spectate, and a disconnected controller has a
30-second lease before the oldest spectator is promoted. Move orders are
canonicalized to the center of a valid tile; fixed-point interpolation carries
units continuously between those lattice waypoints.

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

`aoe-map` is the environment-independent boundary for frozen geographic map
identity. It validates the 600 CE request, physical compression and bounded
virtual dimensions, canonicalizes equivalent ratios and longitudes, and
generates deterministic 32×32 terrain/resource chunks from package inputs.
It deliberately has no filesystem, networking, browser, GDAL, or PROJ
dependency. The configured native preparation worker owns those capabilities,
executes source-backed preparation out of process, and has a separate bounded
request for projected creator-map footprints.
