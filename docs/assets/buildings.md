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

Standing buildings use x1 DE House, Barracks, Outpost and Palisade art,
plus the West European Lumber and Mining Camps (DE has no Dark Age camp).
Town Center retains its reviewed multipart trial art. The Mill composes its
trial body (3483) with all 43 native sail frames (3482), preserving animation
without duplicating the body in the atlas. Native anchors and player
and shadow planes are preserved; player ramps use the selected banner colour. House variants are stable by entity id.
Off-screen Mills do not invalidate frames for animation. Generic
1x1 through 4x4 foundations select one of three stages with authoritative
`progress_permille`. Removed foundations and buildings retain the relevant rubble for three game
seconds; exhausted Farms with no food left leave none.

Farms sample a bounded 256-square region of `de/g_fm1.dds`, a ploughed ground
texture applied to their footprint's
terrain triangles, rather than a billboard. They remain walked on. Builders
use trial task sprite 1496 (five facing rows of fifteen frames), from the
local empires2.dat table's male builder proceeding task. The trial lacks the
farmer sequence: farmers use the forager art, as the explicit fallback.

Construction and Farm behavior, costs, prerequisites, training and research
remain server-owned. Placement previews use resident terrain and resources,
unit positions and building footprints; unloaded map tiles are invalid.
Server refusals still decide races and reachability.
