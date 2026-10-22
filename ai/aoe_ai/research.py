"""Technologies: Loom, the climb to Feudal Age, then economy upgrades."""

from __future__ import annotations

from aoeworld import BuildingKind, Tech, tech_cost

from .constructor import FEUDAL_REQUIREMENT
from .context import Context

#: Researched once Feudal, in this order, each at the building named.
FEUDAL_UPGRADES = (
    (Tech.DOUBLE_BIT_AXE, BuildingKind.LUMBER_CAMP),
    (Tech.HORSE_COLLAR, BuildingKind.MILL),
    (Tech.GOLD_MINING, BuildingKind.MINING_CAMP),
    (Tech.WHEELBARROW, BuildingKind.TOWN_CENTER),
)


class ResearchManager:
    def step(self, ctx: Context) -> None:
        eco = ctx.economy
        center = ctx.center
        if eco is None or center is None:
            return
        villagers = len(ctx.villagers)
        wanted = None
        if not eco.has_researched(Tech.LOOM) and villagers >= 10:
            wanted = (Tech.LOOM, center)
        elif not ctx.feudal and villagers >= ctx.config.feudal_min_villagers:
            if self.feudal_buildings(ctx) >= 2:
                wanted = (Tech.FEUDAL_AGE, center)
        if wanted is not None:
            self.research(ctx, *wanted)
        if ctx.feudal:
            self.upgrades(ctx)

    def feudal_buildings(self, ctx: Context) -> int:
        return sum(1 for b in ctx.buildings if b.complete and b.kind in FEUDAL_REQUIREMENT)

    def research(self, ctx: Context, tech: Tech, building) -> bool:
        key = "research:" + str(tech)
        if building.researching is not None or ctx.memory.cooling(key, ctx.tick):
            return False
        cost = tech_cost(tech)
        if ctx.can_afford(cost):
            if building.queue_length == 0:
                ctx.spend(cost)
                ctx.order("research", building.id, tech)
                ctx.memory.cool(key, ctx.tick, 3 * ctx.world.tick_hz)
                return True
            # Affordable but the queue is busy: stop feeding it so it drains.
            if building.kind == BuildingKind.TOWN_CENTER:
                ctx.hold_town_center = True
        return False

    def upgrades(self, ctx: Context) -> None:
        eco = ctx.economy
        for tech, kind in FEUDAL_UPGRADES:
            if eco.has_researched(tech):
                continue
            builders = [b for b in ctx.buildings if b.kind == kind and b.complete and b.idle]
            if not builders:
                continue
            cost = tech_cost(tech)
            # Keep a float for units: upgrades only from a comfortable stockpile.
            padded = (cost[0] + 150, cost[1] + 100, cost[2], cost[3])
            if ctx.can_afford(padded) and not ctx.memory.cooling("research:" + str(tech), ctx.tick):
                ctx.spend(cost)
                ctx.order("research", builders[0].id, tech)
                ctx.memory.cool("research:" + str(tech), ctx.tick, 5 * ctx.world.tick_hz)
                return
