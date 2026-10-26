"""Fighting: defend the base, mass an army, attack the nearest enemy when
strong, and pull back when the attack is failing."""

from __future__ import annotations

import math
from typing import Any, Optional

from aoeworld import AnimalKind, AnimalTask, Formation

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
        self.protect_workers(ctx)
        self.recover_marches(ctx)
        army = [u for u in ctx.army if u.alive and u.id not in ctx.busy]
        mem = ctx.board
        if not army and any(u.alive for u in ctx.army):
            return
        if self.defend(ctx, army):
            return
        if mem.attacking:
            self.attack(ctx, army)
        elif len(army) >= ctx.config.attack_army_size:
            mem.attacking = True
            mem.attack_launch_size = len(army)
            mem.bump("attacks_launched")
            self.attack(ctx, army)

    def recover_marches(self, ctx: Context) -> None:
        """Release accepted routes that stopped making observable progress.

        Formation members can remain MOVING while waiting on one another.
        Stop only stationary soldiers after the normal stall interval, letting
        the next decision plan a fresh route without interrupting a working one.
        """
        mem = ctx.board
        live = {u.id for u in ctx.army if u.alive}
        mem.march_progress = {i: p for i, p in mem.march_progress.items() if i in live}
        stall = ctx.config.march_stall_seconds * ctx.world.tick_hz
        for unit in ctx.army:
            if not unit.moving or unit.attack_target is not None or unit.id in ctx.busy:
                mem.march_progress.pop(unit.id, None)
                continue
            tile, since = mem.march_progress.get(unit.id, (unit.tile, ctx.tick))
            if dist(tile, unit.tile) >= 2:
                tile, since = unit.tile, ctx.tick
            mem.march_progress[unit.id] = (tile, since)
            if ctx.tick - since >= stall:
                ctx.order("stop", unit.id)
                mem.march_progress.pop(unit.id, None)
                mem.marched.pop(unit.id, None)
                mem.march_goal = None
                mem.bump("marches_recovered")

    # ----- defence ---------------------------------------------------------
    def protect_workers(self, ctx: Context) -> None:
        """Use nearby soldiers or the scout; otherwise workers fight together."""
        cfg = ctx.config
        for animal in ctx.world.animals():
            if animal.hp == 0 or not (
                animal.kind == AnimalKind.WOLF
                or (animal.kind == AnimalKind.BOAR and animal.task == AnimalTask.ATTACKING)
            ):
                continue
            workers = sorted(
                [v for v in ctx.villagers if v.alive
                 and dist(v.tile, animal.tile) <= cfg.predator_worker_radius],
                key=lambda v: dist(v.tile, animal.tile),
            )
            if not workers:
                continue
            defenders = [u for u in ctx.army + ctx.scouts if u.alive
                         and (u.id not in ctx.busy or u.attack_target == animal.id)
                         and self.available_target(ctx, u.id, animal.id)
                         and dist(u.tile, animal.tile) <= cfg.predator_defender_radius]
            defenders = sorted(defenders, key=lambda u: dist(u.tile, animal.tile))[:cfg.predator_soldier_group]
            for defender in defenders or workers[:cfg.predator_worker_group]:
                if defender.attack_target == animal.id:
                    ctx.busy.add(defender.id)
                    continue
                if defender.id in ctx.busy:
                    continue
                if self.available_target(ctx, defender.id, animal.id):
                    ctx.order("attack", defender.id, animal.id)
                    ctx.board.builders.pop(defender.id, None)
                    ctx.board.bump("predator_defences")

    def available_target(self, ctx: Context, unit: int, target: int) -> bool:
        return not ctx.board.cooling(f"attack:{unit}:{target}", ctx.tick)

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
        candidates = sorted((e for e in ctx.enemies if dist(e.tile, ctx.home) <= ctx.config.defend_radius),
                            key=lambda e: (dist(e.tile, ctx.home), e.id))
        # Bound attack-target queries, leaving work for the soldiers' routes.
        # Visible raiders outside this shortlist still count toward danger.
        targets = candidates[:max(1, ctx.config.land_queries_per_decision // 4)]
        near = [e for e in targets if ctx.land_route(ctx.home, e.tile) is not None]
        if not near:
            return False
        # A lone raider (an enemy scout) is not worth abandoning the economy for.
        if len(candidates) >= 3 and len(army) * 2 < len(candidates):
            self.pull_back(ctx, candidates)
        if not army:
            return True
        sent = 0
        for unit in army:
            if sent >= 30:
                break
            if unit.idle or unit.attack_target is None:
                target = nearest([e for e in near if self.available_target(ctx, unit.id, e.id)], unit.tile)
                if target is None:
                    continue
                if ctx.land_route(unit.tile, target.tile) is None:
                    continue
                if dist(unit.tile, target.tile) > ctx.config.march_hop_tiles:
                    self.march(ctx, [unit], unit.tile, target.tile)
                else:
                    ctx.order("attack", unit.id, target.id)
                # Reserve a paused defender too: rally must not override it.
                ctx.busy.add(unit.id)
                sent += 1
        ctx.board.bump("defences")
        return sent > 0

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
    def target_building(self, ctx: Context, origin: Tile, unit: Optional[int] = None) -> Optional[Any]:
        cleared = ctx.board.cleared
        known = sorted((b for b in ctx.world.known_enemy_buildings() if b.id not in cleared and (unit is None or self.available_target(ctx, unit, b.id))),
                       key=lambda b: (dist(origin, b.center), b.id))
        return next((b for b in known if ctx.land_route(origin, b.center) is not None), None)

    def attack(self, ctx: Context, army: list) -> None:
        mem, cfg = ctx.board, ctx.config
        if not army:
            mem.attacking = False
            return
        if len(army) < cfg.retreat_fraction * mem.attack_launch_size:
            mem.attacking = False
            mem.bump("retreats")
            return
        middle = nearest(army, centroid([u.tile for u in army]) or ctx.home).tile
        # Engage visible enemy units close to the army first.
        idle_army = [u for u in army if u.attack_target is None]
        candidates = sorted((e for e in ctx.enemies if any(dist(e.tile, u.tile) <= ENGAGE_RADIUS for u in idle_army)),
                            key=lambda e: (dist(e.tile, middle), e.id))
        candidates = candidates[:max(1, cfg.land_queries_per_decision // 4)]
        reachable_foes = [e for e in candidates if ctx.land_route(middle, e.tile) is not None]
        for unit in army:
            if unit.id in ctx.busy or unit.attack_target is not None:
                continue
            foes = [e for e in reachable_foes if dist(e.tile, unit.tile) <= ENGAGE_RADIUS
                    and self.available_target(ctx, unit.id, e.id)
                    and ctx.land_route(unit.tile, e.tile) is not None]
            if foes:
                ctx.order("attack", unit.id, nearest(foes, unit.tile).id)
        building = self.target_building(ctx, middle)
        # The server forgets a destroyed building when its footprint is seen.
        # Distance from its centre cannot establish sight of a remembered target.
        if building is None:
            self.search(ctx, army, middle)
            return
        mem.attack_target = building.id
        if self.stalled(ctx, middle, building.center):
            mem.cleared.add(building.id)
            mem.bump("targets_unreachable")
            return
        marching: dict[Tile, list] = {}
        for unit in army:
            if not unit.idle or unit.id in ctx.busy:
                continue
            # Shared-origin queries are cached; each march group checks its hop.
            # Per-soldier origins would exhaust the public query budget here.
            target = self.target_building(ctx, middle, unit.id)
            if target is None:
                continue
            if target.visible and dist(unit.tile, target.center) <= cfg.march_hop_tiles:
                ctx.order("attack", unit.id, target.id)
            else:
                goal = target.center if target.visible else self.approach(ctx, target, middle)
                marching.setdefault(goal, []).append(unit)
        for goal, members in marching.items():
            self.march(ctx, members, middle, goal)

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
        if self.stalled(ctx, middle, target):
            mem.search_index += 1
            mem.bump("search_unreachable")
            return
        if not any(u.idle for u in army):
            return
        reachable = ctx.land_frontier(middle, target)
        if reachable is None or reachable == middle:
            mem.search_index += 1
            return
        self.march(ctx, army, middle, reachable)

    def stalled(self, ctx: Context, middle: Tile, goal: Tile) -> bool:
        """True when the army got no nearer `goal` for `march_stall_seconds`."""
        mem = ctx.board
        d = dist(middle, goal)
        if mem.march_goal != goal:
            mem.march_goal, mem.march_best, mem.march_since = goal, d, ctx.tick
            return False
        if d < mem.march_best - 8:
            mem.march_best, mem.march_since = d, ctx.tick
            return False
        return ctx.tick - mem.march_since > ctx.config.march_stall_seconds * ctx.world.tick_hz

    def march(self, ctx: Context, army: list, middle: Tile, goal: Tile) -> None:
        """Nearby soldiers share one short land hop and one server route.
        Stragglers form separate groups so assembly stays local."""
        mem = ctx.board
        pause = ctx.config.march_pause_seconds * ctx.world.tick_hz
        groups: dict[Tile, list] = {}
        side = ctx.config.march_group_tiles
        for unit in army:
            if unit.idle and unit.id not in ctx.busy and ctx.tick - mem.marched.get(unit.id, -pause) >= pause:
                key = (unit.tile[0] // side, unit.tile[1] // side)
                groups.setdefault(key, []).append(unit)
        for members in groups.values():
            members.sort(key=lambda u: u.id)
            origin = centroid([u.tile for u in members]) or middle
            target = ctx.land_hop(origin, goal)
            if target == origin:
                continue
            for unit in members:
                mem.marched[unit.id] = ctx.tick
                mem.march_progress[unit.id] = (unit.tile, ctx.tick)
            if len(members) > 1:
                ctx.order("formation_move", [u.id for u in members], *target,
                          getattr(Formation, ctx.config.army_formation))
            else:
                ctx.order("move", members[0].id, *target)
