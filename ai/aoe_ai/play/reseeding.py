"""Prepay a small farm reserve through ordinary authoritative Mill orders."""
from aoeworld import BuildingKind, building_cost

from ..context import Context


def queue_reseeds(ctx: Context) -> None:
    if not any(b.kind == BuildingKind.FARM for b in ctx.buildings):
        return
    cost = building_cost(BuildingKind.FARM)
    reserve = list(cost)
    reserve[1] += ctx.config.wood_reserve
    for mill in ctx.buildings:
        key = f"reseed:{mill.id}"
        if (mill.kind == BuildingKind.MILL and mill.complete
                and mill.farm_reseeds < ctx.config.farm_reseed_queue
                and not ctx.board.cooling(key, ctx.tick)
                and ctx.can_afford(tuple(reserve))):
            ctx.spend(cost)
            ctx.order("farm_reseed", mill.id, True)
            ctx.board.cool(key, ctx.tick, ctx.config.reseed_settle_seconds * ctx.world.tick_hz)
            return
