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
