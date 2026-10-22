"""Placing buildings: houses, camps, the mill and farms, and production."""

from __future__ import annotations

from typing import Any, Optional

from aoeworld import BuildingKind, ResourceKind, UnitTask, building_cost

from ..context import Context
from . import buildorder
from ..geometry import Tile, dist, nearest
from ..blackboard import Plan

MAX_POPULATION = 200


class Constructor:
    """Owns the plans: a placed foundation is remembered until it is seen."""

    # ----- bookkeeping -----------------------------------------------------
    def refresh(self, ctx: Context) -> None:
        """Forget plans that came true or went stale."""
        timeout = ctx.config.plan_timeout_seconds * ctx.world.tick_hz
        live = []
        for plan in ctx.board.plans:
            seen = any(
                b.kind == plan.kind and abs(b.x - plan.tile[0]) <= 1 and abs(b.y - plan.tile[1]) <= 1
                for b in ctx.buildings
            )
            if not seen and ctx.tick - plan.tick < timeout:
                live.append(plan)
        ctx.board.plans = live

    def pending(self, ctx: Context, kind: BuildingKind) -> int:
        """Foundations of `kind` in progress or planned but not yet seen."""
        unfinished = sum(1 for b in ctx.buildings if b.kind == kind and not b.complete)
        return unfinished + sum(1 for p in ctx.board.plans if p.kind == kind)

    def total(self, ctx: Context, kind: BuildingKind) -> int:
        return ctx.count(kind) + sum(1 for p in ctx.board.plans if p.kind == kind)

    # ----- placing ---------------------------------------------------------
    def place(
        self,
        ctx: Context,
        kind: BuildingKind,
        near: Tile,
        builders: list[Any],
        radius: Optional[int] = None,
        gap: int = 1,
    ) -> bool:
        """Order `builders` to raise a `kind` near `near`; False if impossible now."""
        if not builders:
            return False
        cost = building_cost(kind)
        if not ctx.can_afford(cost):
            return False
        radius = radius or ctx.config.build_search_radius
        site = None
        for extra in (0, 1, 2):
            found = ctx.world.find_build_site(kind, near[0], near[1], radius, gap + extra)
            if found is not None and (str(kind), *found) not in ctx.board.bad_sites:
                site = found
                break
        if site is None:
            return False
        ctx.spend(cost)
        ctx.order("build", [b.id for b in builders], kind, site[0], site[1])
        ctx.board.plans.append(Plan(kind, site, ctx.tick))
        for b in builders:
            ctx.board.builders[b.id] = (str(kind), site)
        ctx.board.bump("build_orders")
        return True

    def builder_near(self, ctx: Context, tile: Tile) -> Optional[Any]:
        """The villager closest to `tile` that is not already building."""
        free = [
            v
            for v in ctx.villagers
            if v.task != UnitTask.BUILDING
            and v.id not in ctx.busy
            and v.id not in ctx.board.farmers.values()
        ]
        return nearest(free, tile)

    def ensure_camp(self, ctx: Context, kind: BuildingKind, node: Any, builder: Any) -> bool:
        """A drop-off of `kind` close to `node`, unless one is on the way."""
        for b in ctx.buildings:
            if b.kind == kind and dist(b.center, node.tile) <= 8:
                return False
        for plan in ctx.board.plans:
            if plan.kind == kind and dist(plan.tile, node.tile) <= 8:
                return False
        if ctx.board.cooling("camp:" + str(kind), ctx.tick):
            return False
        placed = self.place(ctx, kind, node.tile, [builder], radius=7, gap=0)
        if not placed:
            ctx.board.cool("camp:" + str(kind), ctx.tick, 4 * ctx.world.tick_hz)
        return placed

    # ----- periodic needs --------------------------------------------------
    def step(self, ctx: Context) -> None:
        if ctx.economy is None or ctx.home is None:
            return
        self.refresh(ctx)
        self.age_requirements(ctx)
        self.production(ctx)

    def houses(self, ctx: Context) -> None:
        """Raise a house before the population cap is reached."""
        if ctx.economy is None or ctx.home is None:
            return
        eco = ctx.economy
        if eco.population_cap >= MAX_POPULATION:
            return
        production = ctx.count(BuildingKind.BARRACKS) + ctx.count(BuildingKind.ARCHERY_RANGE)
        margin = ctx.config.house_margin + 2 * production
        incoming = 5 * self.pending(ctx, BuildingKind.HOUSE)
        if ctx.free_population() + incoming > margin:
            return
        if self.pending(ctx, BuildingKind.HOUSE) >= 2:
            return
        builder = self.builder_near(ctx, ctx.home)
        if builder is not None and self.place(ctx, BuildingKind.HOUSE, ctx.home, [builder], gap=1):
            ctx.board.bump("houses")

    def age_requirements(self, ctx: Context) -> None:
        """Mill and Lumber Camp early: drop-offs the economy needs and the
        buildings Feudal Age asks for."""
        if len(ctx.villagers) < 8 or ctx.board.cooling("age-buildings", ctx.tick):
            return
        for kind in (BuildingKind.LUMBER_CAMP, BuildingKind.MILL):
            if self.total(ctx, kind) == 0:
                builder = self.builder_near(ctx, ctx.home)
                tile = self.camp_site_hint(ctx, kind)
                if builder is not None and self.place(ctx, kind, tile, [builder], radius=10, gap=1):
                    ctx.board.cool("age-buildings", ctx.tick, 2 * ctx.world.tick_hz)
                    return

    def camp_site_hint(self, ctx: Context, kind: BuildingKind) -> Tile:
        """Near the resource the camp will serve, else beside the Town Center."""
        wanted = ResourceKind.WOOD if kind == BuildingKind.LUMBER_CAMP else ResourceKind.FOOD
        node = nearest(ctx.resources(wanted), ctx.home, key=lambda n: n.tile)
        return node.tile if node is not None else ctx.home

    def production(self, ctx: Context) -> None:
        """Raise the production buildings the build order asks for, one at a time."""
        if ctx.board.cooling("production", ctx.tick):
            return
        for kind, wanted in buildorder.wanted_buildings(ctx):
            if self.total(ctx, kind) >= wanted:
                continue
            needs = buildorder.PREREQUISITE.get(kind)
            if needs is not None and not ctx.count(needs, complete=True):
                continue
            cost = building_cost(kind)
            if not ctx.can_afford((cost[0], cost[1] + ctx.config.wood_reserve, cost[2], cost[3])):
                continue
            builder = self.builder_near(ctx, ctx.home)
            if builder is not None and self.place(ctx, kind, ctx.home, [builder], gap=1):
                ctx.board.cool("production", ctx.tick, 3 * ctx.world.tick_hz)
                return
