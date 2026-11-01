"""A small optional fleet, using only explored public river food and terrain."""

from __future__ import annotations

from typing import Any

from aoeworld import BuildingKind, ResourceKind, UnitKind, unit_cost

from ..context import Context
from ..geometry import dist, nearest
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

    def step(self, ctx: Context) -> None:
        if ctx.economy is None or ctx.home is None:
            return
        fish = [node for node in ctx.resources(ResourceKind.FOOD)
                if node.remaining > 0 and water_food(ctx, node)
                and dist(node.tile, ctx.home) <= ctx.config.max_gather_distance]
        if not fish:
            return
        self.fish(ctx, fish)
        docks = [b for b in ctx.buildings if b.kind == BuildingKind.DOCK]
        if not docks and self.constructor.total(ctx, BuildingKind.DOCK) == 0:
            target = nearest(fish, ctx.home)
            builder = self.constructor.builder_near(ctx, target.tile)
            if builder is not None:
                self.constructor.place(ctx, BuildingKind.DOCK, target.tile, [builder],
                                       radius=ctx.config.dock_search_radius, gap=0)
            return
        # Queue summaries omit unit kinds. One producer keeps the fleet cap
        # exact even when an accepted order has left the in-flight tracker.
        for dock in sorted(docks, key=lambda b: b.id)[:1]:
            if not dock.idle:
                continue
            if any(order.name == "train" and order.args[0] == dock.id
                   and ctx.tick - order.tick < ctx.config.plan_timeout_seconds * ctx.world.tick_hz
                   for order in ctx.board.tracked.values()):
                continue
            if ctx.board.cooling(f"naval-train:{dock.id}", ctx.tick):
                continue
            for kind, wanted in ((UnitKind.FISHING_SHIP, ctx.config.fishing_ships),
                                 (UnitKind.GALLEY, ctx.config.galleys if ctx.feudal else 0)):
                family = (UnitKind.GALLEY, UnitKind.WAR_GALLEY) if kind == UnitKind.GALLEY else (kind,)
                pending = sum(order.name == "train" and order.args[1] in family
                              and ctx.tick - order.tick < ctx.config.plan_timeout_seconds * ctx.world.tick_hz
                              for order in ctx.board.tracked.values())
                if sum(u.kind in family and u.alive for u in ctx.units) + pending >= wanted:
                    continue
                if str(kind) in ctx.board.blocked_units or not ctx.try_spend(unit_cost(kind)):
                    break
                ctx.order("train", dock.id, kind)
                ctx.board.cool(f"naval-train:{dock.id}", ctx.tick,
                               ctx.config.naval_settle_seconds * ctx.world.tick_hz)
                break

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
