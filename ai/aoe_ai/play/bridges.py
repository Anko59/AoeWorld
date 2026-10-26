"""Optional local crossing heuristic, using only explored public terrain."""
from aoeworld import Age, BuildingKind, UnitTask, building_cost
from ..blackboard import Plan
from ..geometry import dist, nearest


class BridgeManager:
    def step(self, ctx):
        if not ctx.config.build_bridges or ctx.economy.age != Age.FEUDAL or not ctx.army:
            return
        if ctx.board.cooling("bridge", ctx.tick):
            return
        ctx.board.cool("bridge", ctx.tick, ctx.config.bridge_retry_seconds * ctx.world.tick_hz)
        if any(b.kind == BuildingKind.BRIDGE and not b.complete for b in ctx.buildings):
            return
        soldier = ctx.army[0]
        targets = sorted(ctx.world.known_enemy_buildings(), key=lambda b: (dist(soldier.tile, b.center), b.id))
        if not targets or ctx.land_route(soldier.tile, targets[0].center) is not None:
            return
        target = targets[0].center
        bank = ctx.land_frontier(soldier.tile, target)
        if bank is None:
            return
        site = ctx.world.find_bridge_site(*bank, *target)
        if site is None:
            return
        start, end = site
        workers = [v for v in ctx.villagers if v.id not in ctx.busy
                   and v.id not in ctx.board.builders and v.task != UnitTask.FARMING]
        worker = nearest(workers, start)
        if worker is None or ctx.land_route(worker.tile, start) is None:
            return
        count = max(abs(end[0] - start[0]), abs(end[1] - start[1])) - 1
        cost = tuple(n * count for n in building_cost(BuildingKind.BRIDGE))
        if not ctx.can_afford(cost):
            return
        ctx.spend(cost)
        ctx.order("build_bridge", [worker.id], *start, *end)
        ctx.board.plans.append(Plan(BuildingKind.BRIDGE, (min(start[0], end[0]), min(start[1], end[1])), ctx.tick))
        ctx.board.builders[worker.id] = (str(BuildingKind.BRIDGE), start)
        ctx.board.bump("bridges")
