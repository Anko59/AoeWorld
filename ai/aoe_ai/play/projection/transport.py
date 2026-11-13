"""A five-passenger ferry, with observed boarding and landing transitions."""
from aoeworld import BuildingKind, UnitKind, unit_cost

from ...geometry import dist


def ferry(manager, ctx, exp, soldiers, worker):
    board, cfg = ctx.board, ctx.config
    ship = ctx.world.unit(exp.transport) if exp.transport else None
    if ship is not None and not ship.alive:
        ship = None
    if ship is None and exp.loaded:
        manager.finish(ctx)
        return
    if ship is None:
        ships = [u for u in ctx.units if u.kind == UnitKind.TRANSPORT_SHIP and u.alive]
        ship = min(ships, key=lambda u: dist(u.tile,exp.origin), default=None)
        if ship:
            exp.transport = ship.id
        else:
            docks = [b for b in ctx.buildings if b.kind == BuildingKind.DOCK]
            if not soldiers:
                manager.finish(ctx)
                return
            if not docks:
                if worker and not manager.constructor.total(ctx,BuildingKind.DOCK):
                    manager.constructor.place(ctx,BuildingKind.DOCK,soldiers[0].tile,[worker],radius=12,gap=0)
                return
            dock = min(docks,key=lambda b:dist(b.center,exp.origin))
            if dock.idle and not any(t.name=='train' and t.args[0]==dock.id for t in board.tracked.values()):
                if ctx.try_spend(unit_cost(UnitKind.TRANSPORT_SHIP)):
                    ctx.order('train',dock.id,UnitKind.TRANSPORT_SHIP)
                    ctx.queued += 1
            return
    manager.focus(ctx,ship.tile)
    if exp.phase == 'board':
        if ship.cargo_count:
            if not exp.loaded:
                board.bump('transport_crossings_started')
                if ship.id not in board.used_transports:
                    board.used_transports.add(ship.id)
                    board.bump('transports_used')
            exp.loaded=True
            exp.departure = exp.departure or ship.tile
            if ship.cargo_count >= len(exp.passengers):
                exp.phase='sail'
            else:
                return
        else:
            exp.passengers=[u.id for u in soldiers[:cfg.projection_ferry_soldiers]]
            if worker:
                exp.passengers.append(worker.id)
            if exp.passengers:
                ctx.order('load',exp.passengers,ship.id)
            return
    if exp.phase == 'sail' and not ship.cargo_count and exp.loaded:
        landed=[ctx.world.unit(i) for i in exp.passengers]
        landed=[u for u in landed if u is not None and u.alive]
        if not landed:
            return
        if min(dist(u.tile,exp.origin) for u in landed) < cfg.projection_stage_distance:
            return
        if exp.departure is None or dist(ship.tile,exp.departure) < cfg.projection_water_distance:
            return
        exp.crew=[u.id for u in landed if u.id!=exp.builder]
        exp.crossed='water'
        exp.proof=dict(kind='water',transport=ship.id,passengers=list(exp.passengers),
                       origin=exp.origin,departure=exp.departure,arrival=ship.tile,
                       landing=landed[0].tile,tick=ctx.tick)
        exp.loaded=False
        exp.phase='advance'
        board.bump('transport_landings')
        return
    # Fixed local shoreline probe. Terrain knowledge is filtered by the SDK.
    shores=[]
    sx,sy=ship.tile
    for dx in range(-8,9):
        for dy in range(-8,9):
            tile=(sx+dx,sy+dy)
            if (exp.departure and dist(ship.tile,exp.departure)>=cfg.projection_water_distance
                    and ctx.world.passable(*tile) is True and dist(tile,exp.origin)>=cfg.projection_stage_distance
                    and dist(tile,exp.goal)+cfg.projection_stage_distance < dist(exp.origin,exp.goal)
                    and tile not in board.bad_landings
                    and any(ctx.world.water_passable(tile[0]+a,tile[1]+b) is True
                            for a,b in ((1,0),(-1,0),(0,1),(0,-1)))):
                shores.append(tile)
    if shores:
        target=min(shores,key=lambda p:dist(p,exp.goal))
        route = ctx.land_route(exp.origin, target)
        if route is None and not ctx.land_unanswered(exp.origin, target):
            ctx.order('unload',ship.id,*target)
            return
    if ship.auto_scout:
        return
    if ctx._land_queries >= cfg.land_queries_per_decision:
        return
    ctx._land_queries += 1
    hop=ctx.world.water_frontier(*ship.tile,*exp.goal)
    if hop and hop!=ship.tile:
        ctx.order('move',ship.id,*hop)
    elif not ship.auto_scout:
        ctx.order('auto_scout',ship.id,True)
