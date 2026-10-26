# Fog of war

Each player has unexplored (black), explored (45% illumination), and visible
tiles. Living owned units, buildings and converted sheep contribute circular
LOS. There is no terrain-height or forest occlusion. Explored tiles persist
after observers move or die. Town Watch adds the existing four building LOS
tiles; Feudal Age raises scout LOS from four to six.

## Storage and work

`game_world/visibility` owns deterministic, ordered indexes. Exploration uses
128-byte bitsets in 32x32 chunks, allocated only after discovery. Current sight
uses reference counts on visible tiles, so overlapping observers compose and
idle/distant tiles have no counter storage. Observers update only when their
tile, owner or LOS changes, or when they spawn/die. Each update removes the old
disk and adds the new disk, with ordered-index lookups per tile. No update
scans or allocates the map. The 256x256 strategic overview is also sparse.

Resource observation probes five possible overlay identities per revealed tile,
without fetching terrain pages. Depletion updates only players currently seeing
that tile. Resource memory is bounded by the overlay's 1,048,576 entries. Reconnect
snapshots use pages of at most 65,536 amounts; the browser publishes the
assembled snapshot atomically.
Buildings retain their last observed state and age variant until revisited; an unseen
destruction stays remembered. Owned building destruction is known immediately.
The existing building count bounds building-state refresh work each tick.

The bounded-cost tests compare 256- and 262,144-tile axes and verify zero LOS
tile work on idle ticks. These are synthetic complexity regressions, not RTS
performance or hardware qualification. Exploration storage grows with discovered
chunks; it is not preallocated in proportion to world dimensions.

Exploration and remembered state enter the replay hash because they depend on
history. Current visibility is derived from live observers. Transport residents,
delivery revisions and instrumentation counters do not enter the replay hash.

## Authority and wire

Each slot's controller receives its own player's view; spectators see the first
slot's. Simulation knowledge is independent per player, and each player has its
own vision revision, so a viewer rebuilds its fog view only when its own sight
changes. A rebuilt view whose region, tiles and overview equal what the client
already holds is not sent again (the client ignores the tick it was built at).
Snapshots, ticks, reconnects and wildlife replacements exclude unseen opponents
and neutral animals. Owned units remain known offscreen. Buildings in fog use
the simulation's remembered snapshots; resource amounts never advance unseen.
Immutable geographic terrain remains accessible through the map service.

`FogState` is appended at server enum index 9, preserving every existing enum index. Building state appends its observed age; current goldens pin
the complete layout. It contains a complete row-major view of the subscription
(at most 512x512 bytes) and at most 65,536 sparse strategic pixels. Both decode
bounds and tile values are validated. Subscription revisions reject stale views;
failed delivery disconnects. The browser encoder and Rust pin the same
`browser/tests/wire/fog.hex` fixture.

Ground illumination uses the existing surface mesh and interpolates shared
vertex levels for soft edges. GPU fog uses the spare high atlas-page byte in
the existing 112-byte packet; Canvas uses the same levels. Remembered buildings
and resources are dimmed. The strategic map overlays the same three states.
Fog shares the current five-page unit-animation atlas and existing WASM gzip gate.

## Values and sources

`make assets-dat-table` now exposes `line_of_sight` in its ignored JSON table
and terminal output. The available `empires2.dat` is the trial's **VER 5.7**,
not a current DE dat file; the decoder does not claim DE-format qualification.
Its Gaia records corroborate these requested base values:

| Observer | Unit id | LOS tiles |
|---|---:|---:|
| Villager | 83 / 293 | 4 |
| Militia | 74 | 4 |
| Scout Cavalry, Dark Age | 448 | 4 |
| Fishing Ship | 13 | 5 |
| Town Center | 109 / 618 | 8 |
| Sheep | 594 | 3 |
| Deer | 65 | 2 |
| Boar | 48 | 4 |
| Wolf | 126 | 12 |

The scout's four-tile base is also documented by
[Forgotten Empires](https://www.forgottenempires.net/strategy/age-of-empires-ii-strategy-center/scouting).
Town Watch's +4 comes from the existing technology table and the requested DE
rules. The requested Militia base remains four. AI acquisition distances stay
separate from animal LOS. Building LOS continues to use `BUILDING_RULES`.
Docks use the current DE eight-tile value documented in [fishing](fishing.md).
Fish use resource memory, including their last observed amounts in explored
fog; enemy Fishing Ships follow the same current-sight filtering as land units.

`AOE_FOG_DISABLED=1` is an authority-side debug/showcase switch. Synthetic
population workloads and Compare's owned terrain viewers explicitly disable
fog. `AOE_FOG_SHOWCASE=1` adds a bounded, initially unseen wolf and enemy Town
Center for scouting and building-memory filming. Ordinary skirmish starts
retain their existing kit.
