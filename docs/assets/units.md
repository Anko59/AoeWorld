# Native DE unit graphics

The VER 8.9 DE DAT is authoritative for every unit kind spawned by the game,
including workers, wildlife, naval units, promotion tiers, the General and
Vassal. `make assets-unit-table-de` regenerates the one current numeric catalog
from the owner's installed DAT. The semantic binding table names DAT unit IDs
and actions; it contains no image names, frame counts or timing constants.

The common reader follows standing, movement, combat, shared work/carry tasks,
death and dead-unit references. Drawable graphic deltas supply parts such as
ship sails, with DAT draw order and hotspot offsets. The importer resolves the
highest installed resolution by sprite identity and retains native cells and
direction rows. Particle-only references are handled by the later effects pass.
No original pixels or source digests enter Git or Docker images.

All unit poses use the same streamed importer and atlas residency path. Native
animation cells are independently compressed in checked local pack chunks.
Only visible cells and bounded lookahead become resident; offscreen cells are
eligible for LRU eviction. Colour and player-mask pages remain paired, preserving
team colour, shadows, native anchors and backend parity. Neutral sheep retain
separate grey colour semantics. See [DE conversion](de-full.md) for budgets.

Playback uses fractional DAT frame durations. Death is a clamped one-shot,
followed by the dead-unit graphic when supplied by the DAT. Server death ticks,
corpse removal and animal food depletion remain authoritative. Ship deaths use
the same lifecycle. The scene packs a native heading and animation phase into
its existing 32-byte DTO; equivalent DAT graphics share compact render tags.

The remaining trial dependencies belong to buildings, terrain, resources,
projectiles, effects and icons. Arrow and javelin conversion is in the subsequent
projectile pass; Mangonel stones already retain their complete native timeline.

## Imperial identities

Two-Handed Swordsman (473), Champion (567), Halberdier (359), Arbalester (492),
Cavalier (283), Paladin (569), Hussar (441), Capped Ram (422), Siege Ram (548)
and Onager (550) each bind their own DAT idle, walk, attack, death and corpse
references. The generated catalog contains 217 semantic poses. General retains
DAT hero 632 and Vassal retains Western King 434; their HUD icons use those
same identities. The DE overlay exports native DDS icons selected by DAT index,
without substituting an earlier unit tier's icon. Arbalester releases at DAT
frame 20, independently of Crossbowman's frame 15.

Identical resolved native recipes share source storage without changing their
semantic unit/pose bindings. Filename, part order, offsets, native cell counts and
timings must all agree. The current unit inventory uses 118,727 cells instead of
123,057; no native cells are sampled away and the 131,072-frame cap is unchanged.

The DE overlay has two required bounded components: world art at the overlay
root and unit art below `actors/`. The importer retains every native cell in
these components, each within the existing source-frame cap. The browser rebases
their page, chunk and recipe indices through the same merge routine. Regenerate
local DE packs after this layout change; no earlier pack layout is retained.

Render tags use 16 bits. Render-only unit heights use `f32`, preserving integer
game levels while keeping the scene record at 32 bytes. Simulation and terrain
height storage are unchanged. Portable tests cover the binding identities,
pose/corpse selection, native cells, icon inventory and exact icon crops.
Runtime browser checks and real-pack visual qualification remain separate;
compilation alone does not establish that the new pixels render correctly.
