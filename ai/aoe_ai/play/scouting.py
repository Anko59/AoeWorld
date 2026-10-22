"""The scout reveals the starting area, then rides the circle around the map
centre that passes through our own base, opposite side first: with villages
spread evenly over the map that is where the enemies are. What it uncovers
lands in the fog memory the SDK keeps; the combat manager targets from there."""

from __future__ import annotations

import math

from ..context import Context
from ..geometry import Tile, clamp_tile, dist, ring_points


class ScoutManager:
    def step(self, ctx: Context) -> None:
        if ctx.home is None:
            return
        board = ctx.board
        for scout in ctx.scouts:
            if not scout.idle:
                continue
            if board.scout_index >= len(board.scout_waypoints):
                self.extend(ctx)
                if board.scout_index >= len(board.scout_waypoints):
                    return
            x, y = board.scout_waypoints[board.scout_index]
            board.scout_index += 1
            ctx.order("move", scout.id, x, y)

    def extend(self, ctx: Context) -> None:
        """Queue the next leg: home ring, map circle, then ever wider rings."""
        board, cfg = ctx.board, ctx.config
        size = ctx.world.map_size
        stage = board.scout_stage
        board.scout_stage += 1
        if stage == 0:
            board.scout_waypoints.extend(
                clamp_tile(p, size) for p in ring_points(ctx.home, 18, spacing=14)
            )
        elif stage == 1:
            board.scout_waypoints.extend(self.map_circle(ctx))
        else:
            radius = cfg.scout_ring_step * (stage - 1)
            if radius > cfg.scout_max_radius:
                board.scout_stage = 1  # start over: the fog may have changed
                return
            board.scout_radius = radius
            board.scout_waypoints.extend(
                clamp_tile(p, size) for p in ring_points(ctx.home, radius, spacing=10)
            )

    def map_circle(self, ctx: Context) -> list[Tile]:
        """Points on the circle round the map centre through home, far ones first."""
        width, height = ctx.world.map_size
        centre = (width / 2, height / 2)
        home = ctx.home
        radius = max(math.dist(home, centre), min(width, height) / 4)
        start = math.atan2(home[1] - centre[1], home[0] - centre[0])
        count = max(8, int(2 * math.pi * radius / 40))
        points = [
            (
                int(centre[0] + radius * math.cos(start + math.tau * i / count)),
                int(centre[1] + radius * math.sin(start + math.tau * i / count)),
            )
            for i in range(1, count)
        ]
        points.sort(key=lambda p: -dist(p, home))
        # Walk the far half outward from the mirror point so the route stays short.
        return [clamp_tile(p, (width, height)) for p in points]
