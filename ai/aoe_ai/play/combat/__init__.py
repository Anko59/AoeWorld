"""Fighting: defend the base, mass an army, attack the nearest enemy when
strong, and pull back when the attack is failing."""

from __future__ import annotations

import math
from typing import Any, Optional

from aoeworld import AnimalKind, AnimalTask, BuildingKind, Formation, UnitTask

from ...context import Context
from ...geometry import Tile, centroid, clamp_tile, dist, near_any, nearest
from . import raid
from .march import march


class CombatManager:
    def step(self, ctx: Context) -> None:
        if ctx.home is None:
            return
        raid.observe(ctx)
        raid.watch_sites(ctx)
        self.protect_workers(ctx)
        self.answer_fire(ctx)
        self.recover_marches(ctx)
        army = [u for u in ctx.army if u.alive and u.id not in ctx.busy]
        mem = ctx.board
        if not army and any(u.alive for u in ctx.army):
            return
        if raid.respond(ctx, army, self.engage, self.pull_back) or self.defend(ctx, army):
            return
        self.engage(ctx, army, ctx.enemies)
        attack_size = ctx.config.attack_army_size
        if mem.counter_owner is not None and ctx.tick < mem.counter_until:
            attack_size = min(attack_size, ctx.config.counter_min_soldiers)
        if mem.attacking:
            self.attack(ctx, self.field_army(ctx, army))
        elif len(army) >= attack_size:
            formed = self.assembled(ctx, army)
            if len(formed) >= attack_size:
                mem.attacking = True
                mem.attack_launch_size = len(formed)
                mem.attack_members = {u.id for u in formed}
                mem.bump("attacks_launched")
                self.attack(ctx, formed)

    def assembled(self, ctx: Context, army: list) -> list:
        """Find the strongest local muster, including troops across cell edges."""
        size = ctx.config.army_muster_tiles
        cells: dict[Tile, list] = {}
        for unit in army:
            x, y = unit.tile
            cells.setdefault((x // size, y // size), []).append(unit)

        def adjacent(cell: Tile):
            x, y = cell
            return (cells.get((x + dx, y + dy), ()) for dx in (-1, 0, 1) for dy in (-1, 0, 1))

        def score(cell: Tile):
            x, y = cell
            return (-sum(len(group) for group in adjacent(cell)),
                    dist((x * size + size // 2, y * size + size // 2), ctx.home), cell)

        return [unit for group in adjacent(min(cells, key=score)) for unit in group]

    def field_army(self, ctx: Context, available: list) -> list:
        members = ctx.board.attack_members
        if not members:
            return available
        field = [u for u in ctx.army if u.alive and u.id in members]
        members.intersection_update(u.id for u in field)
        middle = centroid([u.tile for u in field])
        if middle is not None:
            reinforcements = [u for u in available if u.id not in members
                              and dist(u.tile, middle) <= ctx.config.army_muster_tiles]
            members.update(u.id for u in reinforcements)
            field.extend(reinforcements)
        return field

    def answer_fire(self, ctx: Context) -> None:
        """A soldier hit on the march fights back, or halts when it cannot see
        its attacker, instead of walking on while it is killed."""
        hits = [e for e in ctx.events if e.kind == "under_attack" and not getattr(e, "is_building", False)]
        if not hits:
            return
        soldiers = {u.id: u for u in ctx.army if u.alive}
        enemies = {e.id: e for e in ctx.enemies}
        for event in hits:
            unit = soldiers.get(getattr(event, "id", None))
            if unit is None or unit.id in ctx.busy or unit.attack_target is not None:
                continue
            attacker = enemies.get(getattr(event, "attacker", None))
            if attacker is not None and attacker.alive and self.available_target(ctx, unit.id, attacker.id):
                ctx.order("attack", unit.id, attacker.id)
            elif unit.moving or unit.task == UnitTask.MOVING:
                ctx.order("stop", unit.id)
                ctx.board.marched[unit.id] = ctx.tick
            else:
                continue
            ctx.board.march_progress.pop(unit.id, None)
            ctx.board.bump("fire_answered")

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
        # Soldiers in contact with enemies are held up by the fight, not stuck.
        contact = near_any((e.tile for e in ctx.enemies if e.alive), ctx.config.combat_engage_tiles) \
            if ctx.enemies else (lambda tile: False)
        for unit in ctx.army:
            if ((not unit.moving and unit.task != UnitTask.MOVING)
                    or unit.attack_target is not None or unit.id in ctx.busy or contact(unit.tile)):
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
                mem.cool(f"march-alone:{unit.id}", ctx.tick, 2 * stall)
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

    def engage(self, ctx, army, enemies):
        """Close groups fight enemies they can see without probing a route per soldier."""
        if not army or not enemies:
            return
        cfg = ctx.config
        radius = cfg.combat_engage_tiles
        cells = {}
        for enemy in enemies:
            if enemy.alive:
                x, y = enemy.tile
                group = cells.setdefault((x // radius, y // radius), [])
                if len(group) < cfg.combat_targets_per_cell:
                    group.append(enemy)
        origins = {}
        picks: dict = {}
        limit = ctx.land_query_limit
        ctx.land_query_limit = min(limit, ctx._land_queries + cfg.combat_engage_route_queries)
        try:
            for unit in army:
                if unit.attack_target is not None or unit.id in ctx.busy:
                    continue
                cx, cy = (value // radius for value in unit.tile)
                origin = origins.setdefault((cx, cy), unit.tile)
                targets = (enemy for dx in (-1, 0, 1) for dy in (-1, 0, 1)
                           for enemy in cells.get((cx + dx, cy + dy), ())
                           if dist(unit.tile, enemy.tile) <= radius
                           and self.available_target(ctx, unit.id, enemy.id))
                # A soldier already on the march is under attack-move: the
                # server judges the attack, and only a refusal stops it.
                walking = unit.moving or unit.task == UnitTask.MOVING
                for target in sorted(targets, key=lambda enemy: (dist(unit.tile, enemy.tile), enemy.id)):
                    if walking or ctx.land_route(origin, target.tile) is not None:
                        picks.setdefault((cx, cy), []).append((unit, target))
                        break
        finally:
            ctx.land_query_limit = limit
        formation = getattr(Formation, cfg.army_formation)
        for group in picks.values():
            if len(group) >= cfg.combat_group_min:
                # A dense contact is one attack-move order for the whole group.
                goal = group[0][1].tile
                for offset in range(0, len(group), cfg.march_group_members):
                    if ctx.orders >= ctx.order_limit:
                        return
                    ctx.order("formation_move", [u.id for u, _ in group[offset:offset + cfg.march_group_members]],
                              *goal, formation, attack_move=True)
                continue
            for unit, target in group:
                if ctx.orders >= ctx.order_limit:
                    return
                ctx.order("attack", unit.id, target.id)

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
            if unit.id not in ctx.busy and (unit.idle or unit.attack_target is None):
                target = nearest([e for e in near if self.available_target(ctx, unit.id, e.id)], unit.tile)
                if target is None:
                    continue
                if ctx.land_route(unit.tile, target.tile) is None:
                    continue
                if dist(unit.tile, target.tile) > ctx.config.march_hop_tiles:
                    march(ctx, [unit], unit.tile, target.tile)
                else:
                    ctx.order("attack", unit.id, target.id)
                # Reserve a paused defender too: rally must not override it.
                ctx.busy.add(unit.id)
                sent += 1
        ctx.board.bump("defences")
        return sent > 0

    def pull_back(self, ctx: Context, near: list, *, shelter: Optional[Tile] = None,
                  limit: int = 20) -> None:
        """Order endangered villagers back to a Town Center when outnumbered."""
        shelter = shelter or ctx.home
        target = clamp_tile((shelter[0] + 4, shelter[1] + 4), ctx.world.map_size)
        pulled = 0
        for villager in ctx.villagers:
            if pulled >= limit or ctx.orders >= ctx.order_limit:
                break
            if (villager.id not in ctx.busy and dist(villager.tile, shelter) > 3
                    and any(dist(e.tile, villager.tile) <= 7 for e in near)):
                ctx.order("move", villager.id, *target)
                ctx.board.builders.pop(villager.id, None)
                pulled += 1
        if pulled:
            ctx.board.bump("villagers_pulled_back", pulled)

    # ----- offence ---------------------------------------------------------
    def target_building(self, ctx: Context, origin: Tile, unit: Optional[int] = None) -> Optional[Any]:
        board = ctx.board
        countering = board.counter_owner is not None and board.counter_until > ctx.tick
        known = [b for b in ctx.world.known_enemy_buildings() if not board.cooling(f"target:{b.id}", ctx.tick)
                 and (unit is None or self.available_target(ctx, unit, b.id))]
        if countering:
            bases = [b for b in known if b.owner == board.counter_owner]
            return min(bases, default=None, key=lambda b: (
                b.kind not in (BuildingKind.TOWN_CENTER, BuildingKind.CASTLE),
                dist(origin, b.center), b.id,
            ))
        known.sort(key=lambda b: (b.id != board.attack_target, dist(origin, b.center), b.id))
        if known and known[0].id == board.attack_target:
            return known[0]
        reachable = next((b for b in known[:max(1, ctx.config.land_queries_per_decision // 4)]
                          if ctx.land_route(origin, b.center) is not None), None)
        return reachable or next((b for b in known
                                  if not ctx.world.land_known(*origin) or not ctx.world.land_known(*b.center)), None)

    def attack(self, ctx: Context, army: list) -> None:
        mem, cfg = ctx.board, ctx.config
        if not army:
            mem.attacking = False
            mem.attack_members.clear()
            return
        if len(army) < cfg.retreat_fraction * mem.attack_launch_size:
            mem.attacking = False
            mem.attack_members.clear()
            mem.bump("retreats")
            return
        middle = nearest(army, centroid([u.tile for u in army]) or ctx.home).tile
        building = self.target_building(ctx, middle)
        # The server forgets a destroyed building when its footprint is seen.
        # Distance from its centre cannot establish sight of a remembered target.
        if building is None:
            self.search(ctx, army, middle)
            return
        mem.attack_target = building.id
        if self.stalled(ctx, middle, building.center):
            mem.cool(f"target:{building.id}", ctx.tick, cfg.march_stall_seconds * ctx.world.tick_hz)
            mem.attack_target = mem.march_goal = None
            mem.bump("targets_unreachable")
            return
        marching: dict[Tile, list] = {}
        for unit in army:
            if not unit.idle or unit.id in ctx.busy:
                continue
            # Shared-origin queries are cached; each march group checks its hop.
            # Per-soldier origins would exhaust the public query budget here.
            target = building
            if not self.available_target(ctx, unit.id, building.id):
                target = self.target_building(ctx, middle, unit.id)
                if target is None:
                    continue
            if target.visible and dist(unit.tile, target.center) <= cfg.march_hop_tiles:
                ctx.order("attack", unit.id, target.id)
            else:
                goal = target.center if target.visible else self.approach(ctx, target, middle)
                marching.setdefault(goal, []).append(unit)
        for goal, members in marching.items():
            march(ctx, members, middle, goal)

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
        bearing = mem.counter_bearing
        if bearing and ctx.tick < mem.counter_until:
            scale = min(width, height) / max(1, math.hypot(*bearing))
            aim = tuple(round(a + b * scale) for a, b in zip(ctx.home, bearing))
            points.insert(0, clamp_tile(aim, ctx.world.map_size))
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
        march(ctx, army, middle, target)

    def stalled(self, ctx: Context, middle: Tile, goal: Tile) -> bool:
        """Track motion along detours, including legs away from the goal."""
        mem = ctx.board
        if (mem.march_goal != goal or mem.march_anchor is None
                or dist(middle, mem.march_anchor) >= 8):
            mem.march_goal, mem.march_anchor, mem.march_since = goal, middle, ctx.tick
            return False
        return ctx.tick - mem.march_since > ctx.config.march_stall_seconds * ctx.world.tick_hz
