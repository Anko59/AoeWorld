"""Fighting: defend the base, mass an army, attack the nearest enemy when
strong, and pull back when the attack is failing."""

from __future__ import annotations

import math
from typing import Any, Optional

from ..context import Context
from ..geometry import Tile, centroid, clamp_tile, dist, nearest

#: Threat memories older than this many seconds are forgotten.
THREAT_SECONDS = 20
#: A soldier with nothing to do goes for a visible enemy this close.
ENGAGE_RADIUS = 30


class CombatManager:
    def step(self, ctx: Context) -> None:
        if ctx.home is None:
            return
        self.note_threats(ctx)
        army = [u for u in ctx.army if u.alive]
        mem = ctx.board
        if self.defend(ctx, army):
            return
        if mem.attacking:
            self.attack(ctx, army)
        elif len(army) >= ctx.config.attack_army_size:
            mem.attacking = True
            mem.attack_launch_size = len(army)
            mem.bump("attacks_launched")
            self.attack(ctx, army)

    # ----- defence ---------------------------------------------------------
    def note_threats(self, ctx: Context) -> None:
        for event in ctx.events:
            if event.kind == "under_attack":
                ctx.board.bump("threats_seen")
                ctx.board.threat_until = ctx.tick + THREAT_SECONDS * ctx.world.tick_hz
                ctx.board.threat_tile = (event.x, event.y)

    def threatened(self, ctx: Context) -> bool:
        return ctx.board.threat_until > ctx.tick

    def defend(self, ctx: Context, army: list) -> bool:
        """Engage enemies near home. True when the army is busy defending."""
        near = [e for e in ctx.enemies if dist(e.tile, ctx.home) <= ctx.config.defend_radius]
        if not near:
            return False
        # A lone raider (an enemy scout) is not worth abandoning the economy for.
        if len(near) >= 3 and len(army) * 2 < len(near):
            self.pull_back(ctx, near)
        if not army:
            return True
        sent = 0
        for unit in army:
            if sent >= 30:
                break
            if unit.idle or unit.attack_target is None:
                target = nearest(near, unit.tile)
                ctx.order("attack", unit.id, target.id)
                sent += 1
        ctx.board.bump("defences")
        return True

    def pull_back(self, ctx: Context, near: list) -> None:
        """Outnumbered at home: villagers next to a raider walk to the Town
        Center instead of dying at their work; they return to work when idle."""
        pulled = 0
        for villager in ctx.villagers:
            if pulled >= 20:
                break
            if any(dist(e.tile, villager.tile) <= 7 for e in near) and dist(villager.tile, ctx.home) > 3:
                ctx.order("move", villager.id, ctx.home[0] + 4, ctx.home[1] + 4)
                ctx.board.builders.pop(villager.id, None)
                pulled += 1
        if pulled:
            ctx.board.bump("villagers_pulled_back", pulled)

    # ----- offence ---------------------------------------------------------
    def target_building(self, ctx: Context, origin: Tile) -> Optional[Any]:
        known = ctx.world.known_enemy_buildings()
        return nearest(known, origin, key=lambda b: b.center)

    def attack(self, ctx: Context, army: list) -> None:
        mem, cfg = ctx.board, ctx.config
        if not army:
            mem.attacking = False
            return
        if len(army) < cfg.retreat_fraction * mem.attack_launch_size:
            mem.attacking = False
            mem.bump("retreats")
            return
        middle = centroid([u.tile for u in army]) or ctx.home
        # Engage visible enemy units close to the army first.
        engaged = 0
        for unit in army:
            if not unit.idle:
                continue
            foes = [e for e in ctx.enemies if dist(e.tile, unit.tile) <= ENGAGE_RADIUS]
            if foes:
                ctx.order("attack", unit.id, nearest(foes, unit.tile).id)
                engaged += 1
        building = self.target_building(ctx, ctx.home)
        # The server forgets a destroyed building when its footprint is seen.
        # Distance from its centre cannot establish sight of a remembered target.
        if building is None:
            self.search(ctx, army, middle)
            return
        mem.attack_target = building.id
        for unit in army:
            if not unit.idle:
                continue
            if building.visible:
                ctx.order("attack", unit.id, building.id)
            else:
                x, y = self.approach(ctx, building, middle)
                ctx.order("move", unit.id, x, y)

    def approach(self, ctx: Context, building: Any, origin: Tile) -> Tile:
        """A tile just outside the building toward `origin`: a move onto the
        building's own footprint is refused, but standing in sight starts the
        attack."""
        cx, cy = building.center
        reach = max(building.width, building.height) / 2 + 3
        d = max(1.0, dist((cx, cy), origin))
        bad = ctx.board.bad_moves
        first = None
        # A tile the server refused (a tree, a wall, water) is walked around.
        for extra in (0, 2, 4, 6):
            for turn in (0.0, 0.6, -0.6, 1.2, -1.2):
                ux, uy = (origin[0] - cx) / d, (origin[1] - cy) / d
                cos, sin = math.cos(turn), math.sin(turn)
                point = clamp_tile(
                    (
                        round(cx + (ux * cos - uy * sin) * (reach + extra)),
                        round(cy + (ux * sin + uy * cos) * (reach + extra)),
                    ),
                    ctx.world.map_size,
                )
                first = first or point
                if point not in bad:
                    return point
        return first

    def search(self, ctx: Context, army: list, middle: Tile) -> None:
        """No enemy building known: march across the map, the far side first, then
        round the centre, moving on whenever the army has arrived and found nothing."""
        mem = ctx.board
        width, height = ctx.world.map_size
        far = (width - 1 - ctx.home[0], height - 1 - ctx.home[1])
        ring = [
            (int(width / 2 + 0.33 * width * math.cos(a * math.pi / 4)), int(height / 2 + 0.33 * height * math.sin(a * math.pi / 4)))
            for a in range(8)
        ]
        points = [far] + sorted(ring, key=lambda p: -dist(p, ctx.home))
        target = ctx.unrefused(clamp_tile(points[mem.search_index % len(points)], ctx.world.map_size))
        if dist(middle, target) <= 10:
            mem.search_index += 1
            mem.bump("search_legs")
            target = ctx.unrefused(clamp_tile(points[mem.search_index % len(points)], ctx.world.map_size))
        for unit in army:
            if unit.idle:
                ctx.order("move", unit.id, target[0], target[1])
