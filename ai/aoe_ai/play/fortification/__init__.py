"""Income-sized defences on public resource sites and known enemy approaches."""
from aoeworld import BuildingKind, building_cost, building_size

from ...blackboard import Plan
from ...geometry import clamp_tile, dist


def frontier(ctx):
    """The enemy-facing edge of the nearest surviving settlement."""
    enemies = ctx.world.known_enemy_buildings()
    if not enemies:
        return None
    center, enemy = min(((c, e.center) for c in ctx.centers for e in enemies),
                        key=lambda pair: dist(*pair), default=(None, None))
    if center is None:
        return None
    length = max(1, dist(center, enemy))
    reach = ctx.config.fort_frontier_distance
    return clamp_tile(tuple(round(c + (e - c) * reach / length)
                            for c, e in zip(center, enemy)), ctx.world.map_size)


class FortificationManager:
    def __init__(self, constructor):
        self.constructor = constructor

    def lane(self, ctx, center, rejected=()):
        """Prefer a narrow explored cross-section of a reachable approach.

        Unknown terrain is never counted as a wall. The local probe has a fixed
        size and the public land queries share the decision's existing budget.
        """
        enemies = ctx.world.known_enemy_buildings()
        if not enemies:
            return None
        goal = min(enemies, key=lambda e: dist(center, e.center)).center
        length = max(1, dist(center, goal))
        dx, dy = goal[0] - center[0], goal[1] - center[1]
        across = (0, 1) if abs(dx) > abs(dy) else (1, 0)
        candidates = []
        gate_width = building_size(BuildingKind.STONE_GATE)[0]
        passable = getattr(ctx.world, 'passable', None)
        for reach in ctx.config.fort_lane_distances:
            point = clamp_tile((round(center[0] + dx * reach / length),
                                round(center[1] + dy * reach / length)), ctx.world.map_size)
            if passable is not None:
                cross = [passable(point[0] + across[0] * n, point[1] + across[1] * n)
                         for n in range(-ctx.config.fort_wall_wing, ctx.config.fort_wall_wing + 1)]
                if any(tile is None for tile in cross) or passable(*point) is not True:
                    continue
                width = sum(tile is True for tile in cross)
                # The gate must fit the opening, and its own footprint must be known open.
                if width < gate_width or any(passable(point[0] - 2 + n, point[1]) is not True
                                             for n in range(gate_width)):
                    continue
            else:
                width = 2 * ctx.config.fort_wall_wing + 1
            if point in rejected:
                continue
            if ctx.land_route(center, point) is not None:
                candidates.append((width, abs(reach - ctx.config.fort_lane_distances[1]), point))
        return min(candidates)[2] if candidates else None

    def step(self, ctx):
        cfg, board = ctx.config, ctx.board
        if (not cfg.fortifications or not ctx.castle or ctx.home is None
                or len(ctx.villagers) < cfg.fort_workers_per_tower
                or board.cooling('fortify', ctx.tick)):
            return
        board.cool('fortify', ctx.tick, cfg.fort_interval_seconds * ctx.world.tick_hz)
        towers = [b.center for b in ctx.buildings if b.kind == BuildingKind.WATCH_TOWER]
        towers += [p.tile for p in board.plans if p.kind == BuildingKind.WATCH_TOWER]
        cap = max(1, len(ctx.villagers) // cfg.fort_workers_per_tower)
        # Resource drop-offs are paid evidence of economic activity, including
        # remote mines and forests. Do not build speculative towers in fog.
        targets = [(b.center, cfg.fort_site_radius) for b in ctx.buildings if b.complete and b.kind in (
            BuildingKind.MINING_CAMP, BuildingKind.LUMBER_CAMP, BuildingKind.MILL)]
        center = (ctx.centers or [ctx.home])[board.fort_cursor % max(1, len(ctx.centers))]
        board.fort_cursor += 1
        # Retain a layout while it is being built; new scouting must not move
        # the gate between its two wings. Forget lost settlements.
        board.fort_lanes = {c: lane for c, lane in board.fort_lanes.items() if c in ctx.centers}
        board.fort_rejected = {c: r for c, r in board.fort_rejected.items() if c in ctx.centers}
        lane = board.fort_lanes.get(center)
        if lane is None:
            lane = self.lane(ctx, center, board.fort_rejected.get(center, ()))
            if lane is not None:
                board.fort_lanes[center] = lane
        if lane is not None:
            axis = 0 if abs(lane[0] - center[0]) > abs(lane[1] - center[1]) else 1
            tower = list(lane)
            tower[axis] -= cfg.fort_lane_tower_offset * (1 if lane[axis] > center[axis] else -1)
            targets.insert(0, (tuple(tower), cfg.fort_lane_tower_radius))
        if len(towers) < cap:
            for target, radius in targets:
                if any(dist(target, tower) <= cfg.fort_tower_coverage for tower in towers):
                    continue
                if self.place(ctx, BuildingKind.WATCH_TOWER, target, radius=radius):
                    return
        if lane is not None and len(ctx.villagers) >= cfg.fort_wall_workers:
            self.wall(ctx, center, lane)

    def place(self, ctx, kind, target, exact=False, radius=None):
        cfg = ctx.config
        self.refused = False
        if ctx.board.cooling('world-limit:build', ctx.tick):
            return False
        cost = building_cost(kind)
        # Leave an essential wood float and the next Town Center's stone.
        if not ctx.can_afford((cost[0], cost[1] + cfg.fort_wood_float,
                               cost[2], cost[3] + cfg.fort_stone_float)):
            return False
        builder = self.constructor.builder_near(ctx, target)
        if builder is None:
            return False
        queries = ctx._site_queries
        site = ctx.find_build_site(kind, *target, 1 if exact else (radius or cfg.fort_site_radius), 0)
        if (site is None or (exact and site != target)
                or (str(kind), *site) in ctx.board.bad_sites):
            # An exhausted query budget says nothing about the site itself.
            self.refused = ctx._site_queries > queries or (
                site is not None and (str(kind), *site) in ctx.board.bad_sites)
            return False
        width, height = building_size(kind)
        x, y = site
        edges = ((x - 1, y), (x + width, y), (x, y - 1), (x, y + height))
        if not any(ctx.land_route(builder.tile, edge) is not None
                   for edge in sorted(edges, key=lambda tile: dist(builder.tile, tile))):
            self.refused = True
            return False
        ctx.spend(cost)
        ctx.order('build', [builder.id], kind, *site)
        ctx.board.plans.append(Plan(kind, site, ctx.tick))
        ctx.board.builders[builder.id] = (str(kind), site)
        ctx.board.bump('fortifications_placed')
        return True

    def wall(self, ctx, center, lane):
        """One exact footprint at a time, gate first, preserving worker access.

        The ordinary Build command makes a horizontal four-tile gate. For an
        east/west threat its wings turn north/south at opposite gate ends,
        making a stepped line across the approach without a closed opening.
        """
        x, y = lane
        origin = (x - 2, y)
        gate = next((b for b in ctx.buildings if b.kind == BuildingKind.STONE_GATE
                     and (b.x, b.y) == origin), None)
        planned = {(p.kind, p.tile) for p in ctx.board.plans}
        if gate is None:
            if (BuildingKind.STONE_GATE, origin) not in planned:
                if not self.place(ctx, BuildingKind.STONE_GATE, origin, exact=True) and self.refused:
                    # Unbuildable here: forget this lane so the next sampled one is tried.
                    ctx.board.fort_lanes.pop(center, None)
                    ctx.board.fort_rejected.setdefault(center, set()).add(lane)
            return
        if not gate.complete:
            return
        vertical = abs(lane[0] - center[0]) > abs(lane[1] - center[1])
        length = ctx.config.fort_wall_wing
        tiles = ([(x - 2, y - i) for i in range(1, length + 1)]
                 + [(x + 1, y + i) for i in range(1, length + 1)] if vertical else
                 [(x - 2 - i, y) for i in range(1, length + 1)]
                 + [(x + 1 + i, y) for i in range(1, length + 1)])
        occupied = {(b.x, b.y) for b in ctx.buildings if b.kind in (
            BuildingKind.STONE_WALL, BuildingKind.STONE_GATE)}
        for tile in tiles:
            if tile in occupied or (BuildingKind.STONE_WALL, tile) in planned:
                continue
            if self.place(ctx, BuildingKind.STONE_WALL, tile, exact=True):
                return
