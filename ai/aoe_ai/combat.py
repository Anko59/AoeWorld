"""Fighting: defend the base, mass an army, attack the nearest enemy when
strong, and pull back when the attack is failing."""

from __future__ import annotations

from typing import Any, Optional

from .context import Context
from .geometry import Tile, centroid, dist, nearest

#: Threat memories older than this many seconds are forgotten.
THREAT_SECONDS = 20


class CombatManager:
    def step(self, ctx: Context) -> None:
        if ctx.home is None:
            return
        self.note_threats(ctx)
        army = [u for u in ctx.army if u.alive]
        mem = ctx.memory
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
                ctx.memory.bump("threats_seen")
                ctx.memory.threat_until = ctx.tick + THREAT_SECONDS * ctx.world.tick_hz
                ctx.memory.threat_tile = (event.x, event.y)

    def threatened(self, ctx: Context) -> bool:
        return ctx.memory.threat_until > ctx.tick

    def defend(self, ctx: Context, army: list) -> bool:
        """Engage enemies near home. True when the army is busy defending."""
        near = [e for e in ctx.enemies if dist(e.tile, ctx.home) <= ctx.config.defend_radius]
        if not near:
            return False
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
        ctx.memory.bump("defences")
        return True

    # ----- offence ---------------------------------------------------------
    def target_building(self, ctx: Context, origin: Tile) -> Optional[Any]:
        known = ctx.world.known_enemy_buildings()
        return nearest(known, origin, key=lambda b: b.center)

    def attack(self, ctx: Context, army: list) -> None:
        mem, cfg = ctx.memory, ctx.config
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
            foes = [e for e in ctx.enemies if dist(e.tile, unit.tile) <= 9]
            if foes:
                ctx.order("attack", unit.id, nearest(foes, unit.tile).id)
                engaged += 1
        building = self.target_building(ctx, ctx.home)
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
                ctx.order("move", unit.id, building.center[0], building.center[1])

    def search(self, ctx: Context, army: list, middle: Tile) -> None:
        """No enemy building known: march to the map's far side to find one."""
        width, height = ctx.world.map_size
        target = (width - 1 - ctx.home[0], height - 1 - ctx.home[1])
        for unit in army:
            if unit.idle:
                ctx.order("move", unit.id, target[0], target[1])
