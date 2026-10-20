# Dark Age building art

`make assets-import-de AOE_ASSET_PACK=local-assets/packs/<trial-hash>` imports
owner-licensed Definitive Edition art and attaches its pack at the trial pack's
ignored `buildings` link. `DE_DIR` defaults to the owner's Steam installation.
Start `make dev DEV_PORT=8092 AOE_ASSET_PACK=local-assets/packs/<trial-hash>`.
Neither the install, packs, link nor captures belong in Git or container images.

The client loads the trial pack and this optional overlay into the same three
2048-square runtime pages. The unchanged 2560-frame ceiling and allocation
remain enforced by the shared packing planner. Idle actors retain only the
first native frame per facing (the only frame previously drawn); walking and
working animation rows remain complete. This frees atlas space without
removing displayed animation frames. An absent overlay permits
semantic building markers; corrupt present sources fail startup.

Standing buildings use the trial DAT composition whenever its primary stand
is available, with native deltas, annex positions and foundation terrain.
The optional x1 DE overlay supplies missing main bodies, including the West
European Lumber and Mining Camps (DE has no Dark Age camp). It also supplies
rubble and optional Farm ground texturing. The Mill composes its
trial body (3483) with all 43 native sail frames (3482), preserving animation
without duplicating the body in the atlas. Native anchors and player
and shadow planes are preserved; player ramps use the selected banner colour. House variants are stable by entity id.
Off-screen Mills do not invalidate frames for animation. Generic
1x1 through 4x4 supplemental foundations retain their three stages; when DAT
construction art is present, its frame/angle layout and offsets instead select
the stages with authoritative
`progress_permille`. Removed foundations and buildings retain the relevant rubble for three game
seconds; exhausted Farms with no food left leave none.

Without the DE overlay, completed Farms retain the imported native ploughed
sheet (SLP 15004) on their footprint's terrain triangles. The optional overlay
replaces it with a bounded 256-square region of `de/g_fm1.dds`. Neither path
uses a billboard; Farms remain walked on. Builders
use trial task sprite 1496 (five facing rows of fifteen frames), from the
local empires2.dat table's male builder proceeding task. The trial lacks the
farmer sequence: farmers use the forager art, as the explicit fallback.

Construction and Farm behavior, costs, prerequisites, training and research
remain server-owned. Dirt covers the footprint plus one tile on each side,
including construction sites and buildings absent from the trial art. The
decoded AoK building tail declares the footprint and foundation terrain but no
margin; one tile is a presentation choice, not a measured original-engine rule.
DAT footprints and foundation materials take priority. Farm margins use dirt;
completed crop terrain stays inside its footprint. The renderer computes displayed
terrain bounds once and rejects distant buildings before painting. One scan gives
footprints priority over every apron, so neighbours cannot erase crops.
The prepared scene retains its foundation key; building updates check it once,
while unchanged per-actor and preview projections reuse the scene directly.
Completed crops keep their custom
triangle UV mapping rather than inheriting the terrain mixer's tiled mode.
Foundation dirt tiles retain landscape
appearance metadata with canopy and litter cleared. Clearing that metadata
previously excluded them from the noisy material mixer, producing hard edges.
The existing world-anchored terrain shares now blend both sides of each edge
through the same shaders and Canvas kernel as ordinary ground materials.
Placement previews use resident terrain and resources,
unit positions and building footprints; unloaded map tiles are invalid.
Server refusals still decide races and reachability.

## Town Center orientation evidence

`make assets-de-reference` decodes the complete
`b_dark_town_center_age1_x1.sld` from the read-only DE install. It writes
`.cache/tc-shots/reference-de.png` and `orientation-check.png` (DE left, trial
right), using the same 96x48 source tile, ground origin and player colour as
`make assets-building-preview`. Run the latter first; its `tc-composite.png`
is the comparison input. All proof pixels remain ignored.

The complete DE sprite confirms the trial's right wing roof direction: both
wing ridges run parallel to the central roof. There is no reversed right-wing
selection in this comparison. The apparent direction is authored into the
native sprite, not produced by a rotation. The trial's native anchors also
carry horizontal placement; annex tile displacements alone are not sprite
positions. The DAT chain is:

| Unit | Stand graphic / body SLP | Delta graphic / attachment SLP | Tile displacement |
| --- | --- | --- | --- |
| 109 (head) | 3241 / 3596 | 5470 / 4612 | (0, 0) |
| 619 | 3240 / 3595 | 5469 / 4611 | (-0.5, 0.5) |
| 620 | 3239 / 3594 | 5468 / 4610 | (-1, 1) |
| 618 (main) | 435 / 891 | none | (1, -1) |

These six body/attachment SLPs each contain exactly one frame; DAT also declares
one frame and one angle. Their deltas have display angle -1 and zero pixel
offsets. The right roof is SLP 4610, 142x115 pixels with native anchor
(-48,120); that negative X anchor places its pixels to the right of annex 620's
ground origin. Runtime UV widths stay positive. Swapping graphics, selecting another
angle or mirroring them would contradict this evidence. SLP 890 is absent from
the trial archive and referenced on layer 10, below the layer-20 bodies; its
absence supplies no alternate roof orientation. `make test-assets-dat` pins
the unit/stand/attachment selections and single-frame source inventory. The
DE and AoK artwork differs in detail, so this is an orientation check rather
than pixel equivalence or an original-executable trace.
