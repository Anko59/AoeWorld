"""Technologies: Loom, the climb to Feudal Age, then the build order's upgrades."""

from __future__ import annotations

from aoeworld import BuildingKind, Tech, tech_cost

from ..context import Context
from . import buildorder


class ResearchManager:
    def step(self, ctx: Context) -> None:
        eco = ctx.economy
        center = ctx.center
        if eco is None or center is None:
            return
        stage = buildorder.stage(ctx)
        if stage == buildorder.ADVANCE:
            # Everything for the age is standing: stop spending on anything but
            # the age and let food pile up to its price.
            if not self.research(ctx, Tech.FEUDAL_AGE, center):
                ctx.hold_town_center = True
        elif not eco.has_researched(Tech.LOOM) and buildorder.wants_loom(ctx):
            self.research(ctx, Tech.LOOM, center)
        if ctx.feudal:
            self.upgrades(ctx)

    def research(self, ctx: Context, tech: Tech, building) -> bool:
        key = "research:" + str(tech)
        if building.researching is not None or ctx.board.cooling(key, ctx.tick):
            return False
        cost = tech_cost(tech)
        if ctx.can_afford(cost):
            if building.queue_length == 0:
                ctx.spend(cost)
                ctx.order("research", building.id, tech)
                ctx.board.cool(key, ctx.tick, 3 * ctx.world.tick_hz)
                return True
            # Affordable but the queue is busy: stop feeding it so it drains.
            if building.kind == BuildingKind.TOWN_CENTER:
                ctx.hold_town_center = True
        return False

    def upgrades(self, ctx: Context) -> None:
        eco = ctx.economy
        for tech, kind in buildorder.wanted_upgrades(ctx):
            if eco.has_researched(tech) or ctx.board.cooling("research:" + str(tech), ctx.tick):
                continue
            builders = [b for b in ctx.buildings if b.kind == kind and b.complete and b.idle]
            if not builders:
                continue
            cost = tech_cost(tech)
            # Keep a float for units: upgrades only from a comfortable stockpile.
            padded = (cost[0] + 100, cost[1] + 75, cost[2], cost[3])
            if ctx.can_afford(padded):
                ctx.spend(cost)
                ctx.order("research", builders[0].id, tech)
                ctx.board.cool("research:" + str(tech), ctx.tick, 5 * ctx.world.tick_hz)
                return
