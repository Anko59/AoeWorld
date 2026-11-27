"""A small optional fleet, using only explored public river food and terrain."""

from __future__ import annotations

from typing import Any

from aoeworld import BuildingKind, ResourceKind, UnitKind, building_cost

from ..context import Context
from ..geometry import dist, nearest
from ..roster import best_recruit, options, price, visible_composition, warship
from . import buildorder
from .constructor import Constructor


def water_food(ctx: Context, node: Any) -> bool:
    passable = getattr(ctx.world, "passable", None)
    return (node.kind == ResourceKind.FOOD and passable is not None
            and passable(*node.tile) is False)


def food_reachable(ctx: Context, villager: Any, node: Any) -> bool:
    """Fish are reached from an adjacent bank, never by a land route into water."""
    if not water_food(ctx, node):
        return ctx.land_route(villager.tile, node.tile) is not None
    x, y = node.tile
    banks = [(x + dx, y + dy) for dx in (-1, 0, 1) for dy in (-1, 0, 1)
             if (dx or dy) and ctx.world.passable(x + dx, y + dy) is True]
    return any(ctx.land_route(villager.tile, bank) is not None
               for bank in sorted(banks, key=lambda tile: dist(tile, villager.tile)))


class NavalManager:
    def __init__(self, constructor: Constructor) -> None:
        self.constructor = constructor

    def can_fund_fishing(self, ctx: Context, cost: tuple[int, int, int, int]) -> bool:
        food, wood, gold, stone = cost
        return ctx.can_afford((food, wood + buildorder.fleet_wood_reserve(ctx), gold, stone))

    def step(self, ctx: Context) -> None:
        if ctx.economy is None or ctx.home is None:
            return
        fish = [node for node in ctx.resources(ResourceKind.FOOD)
                if node.remaining > 0 and water_food(ctx, node)
                and dist(node.tile, ctx.home) <= ctx.config.max_gather_distance]
        if fish:
            self.fish(ctx, fish)
        dock = min((b for b in ctx.buildings if b.kind == BuildingKind.DOCK and b.complete),
                   key=lambda b: b.id, default=None)
        if dock is None:
            if (fish and self.constructor.total(ctx, BuildingKind.DOCK) == 0
                    and self.can_fund_fishing(ctx, building_cost(BuildingKind.DOCK))):
                target = nearest(fish, ctx.home)
                builder = self.constructor.builder_near(ctx, target.tile)
                if builder is not None:
                    self.constructor.place(ctx, BuildingKind.DOCK, target.tile, [builder],
                                           radius=ctx.config.dock_search_radius, gap=0)
            return
        if not dock.idle or dock.id in ctx.draining or ctx.free_population() <= 0:
            return
        if any(order.name == "train" and order.args[0] == dock.id
               for order in ctx.board.tracked.values()):
            return
        key = f"naval-train:{dock.id}"
        if ctx.board.cooling(key, ctx.tick):
            return
        kind = self.recruit(ctx, dock, fish)
        if kind is not None and ctx.try_spend(price(kind)):
            ctx.order("train", dock.id, kind)
            ctx.queued += 1
            ctx.board.cool(key, ctx.tick, ctx.config.naval_settle_seconds * ctx.world.tick_hz)

    def recruit(self, ctx: Context, dock: Any, fish: list):
        fishing = UnitKind.FISHING_SHIP
        if (fish and sum(u.alive and u.kind == fishing for u in ctx.units) < ctx.config.fishing_ships
                and self.can_fund_fishing(ctx, price(fishing))
                and str(fishing) not in ctx.board.blocked_units):
            return fishing
        if not ctx.feudal:
            return None
        enemies = visible_composition(
            (e for e in ctx.enemies if dist(e.tile, dock.center) <= ctx.config.fort_threat_radius),
            warship,
        )
        if not enemies:
            return None
        have = ctx.combat_lines
        if sum(count for kind, count in have.items() if warship(kind)) >= ctx.config.warships:
            return None
        choices = (kind for kind in options(dock.kind, ctx.economy.age, tuple(ctx.economy.researched))
                   if warship(kind))
        return best_recruit(ctx, choices, have, enemies)

    def fish(self, ctx: Context, nodes: list) -> None:
        for ship in ctx.units:
            if (ship.kind != UnitKind.FISHING_SHIP or not ship.alive or not ship.idle
                    or ship.id in ctx.busy or ctx.board.cooling(f"fishing:{ship.id}", ctx.tick)):
                continue
            target = nearest(nodes, ship.tile)
            if target is not None:
                ctx.order("gather", ship.id, target.id)
                ctx.board.cool(f"fishing:{ship.id}", ctx.tick,
                               ctx.config.naval_settle_seconds * ctx.world.tick_hz)
