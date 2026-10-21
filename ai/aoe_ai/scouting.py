"""The scout circles the base in widening rings, then heads for where an
enemy probably is. Whatever it uncovers lands in the fog memory the
SDK keeps; buildings it sees are known to the combat manager."""

from __future__ import annotations

from .context import Context
from .geometry import Tile, clamp_tile, ring_points


class ScoutManager:
    def step(self, ctx: Context) -> None:
        if ctx.home is None:
            return
        mem = ctx.memory
        for scout in ctx.scouts:
            if not scout.idle:
                continue
            if mem.scout_index >= len(mem.scout_waypoints):
                self.extend(ctx)
                if mem.scout_index >= len(mem.scout_waypoints):
                    return
            x, y = mem.scout_waypoints[mem.scout_index]
            mem.scout_index += 1
            ctx.order("move", scout.id, x, y)

    def extend(self, ctx: Context) -> None:
        """Queue the next ring, interleaved with a probe toward the far side."""
        mem, cfg = ctx.memory, ctx.config
        radius = mem.scout_radius + cfg.scout_ring_step
        if radius > cfg.scout_max_radius:
            # Everything near is seen; sweep the mirror image of home.
            mem.scout_radius = 0
            mem.scout_waypoints.extend(self.far_probe(ctx))
            return
        mem.scout_radius = radius
        size = ctx.world.map_size
        mem.scout_waypoints.extend(
            clamp_tile(p, size) for p in ring_points(ctx.home, radius, spacing=10)
        )

    def far_probe(self, ctx: Context) -> list[Tile]:
        width, height = ctx.world.map_size
        mirror = (width - 1 - ctx.home[0], height - 1 - ctx.home[1])
        return [clamp_tile(p, ctx.world.map_size) for p in ring_points(mirror, 18, spacing=10)]
