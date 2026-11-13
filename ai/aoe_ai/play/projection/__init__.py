"""One bounded expedition: muster, cross, establish production, then attack."""
from dataclasses import dataclass, field
from heapq import nsmallest

from aoeworld import BuildingKind, Formation, Tech, UnitKind, building_cost, unit_cost

from ...blackboard import Plan
from ...context import MILITARY
from ...geometry import clamp_tile, dist
from .transport import ferry


@dataclass
class Expedition:
    origin: tuple
    goal: tuple
    crew: list
    builder: int
    phase: str = 'stage'
    target: int = None
    transport: int = None
    crossed: str = None
    passengers: list = field(default_factory=list)
    loaded: bool = False
    started: int = 0
    sites: dict = field(default_factory=dict)
    bridge: tuple = None
    deck_seen: set = field(default_factory=set)
    proof: dict = field(default_factory=dict)
    departure: tuple = None
    probe: int = 0


class ProjectionManager:
    def __init__(self, constructor):
        self.constructor = constructor

    def prepare(self, ctx):
        cfg, board = ctx.config, ctx.board
        exp = board.expedition
        if (exp is None and cfg.force_projection and ctx.castle and ctx.home
                and ctx.tick >= cfg.projection_start_seconds * ctx.world.tick_hz
                and len(ctx.army) >= cfg.projection_army_size):
            if board.cooling('expedition', ctx.tick):
                return
            targets = [b for b in ctx.world.known_enemy_buildings() if b.id not in board.cleared
                       and ctx.settlement_distance(b.center) >= cfg.projection_distance]
            target = min(targets, key=lambda b: (b.kind in (BuildingKind.CASTLE, BuildingKind.TOWN_CENTER),
                                                dist(ctx.home, b.center), b.id), default=None)
            goal = target.center if target else (ctx.world.map_size[0] // 2, ctx.world.map_size[1] // 2)
            if dist(ctx.home, goal) < cfg.projection_distance:
                goal = (ctx.world.map_size[0] - 1 - ctx.home[0], ctx.world.map_size[1] - 1 - ctx.home[1])
            worker = self.constructor.builder_near(ctx, ctx.home)
            if worker is None:
                return
            crew = nsmallest(cfg.projection_army_size, ctx.army, key=lambda u: (dist(u.tile, ctx.home), u.id))
            exp = Expedition(ctx.home, goal, [u.id for u in crew], worker.id,
                             target=target.id if target else None, started=ctx.tick)
            board.expedition = exp
            board.bump('expeditions_launched')
        if exp is None:
            return
        # Fixed group; never emit or query once per member of a growing army.
        ctx.busy.update(exp.crew)
        ctx.busy.add(exp.builder)
        if exp.bridge and not exp.crossed:
            a, b = exp.bridge
            for identity in exp.crew + [exp.builder]:
                u = ctx.world.unit(identity)
                if u and min(a[0], b[0]) <= u.tile[0] <= max(a[0], b[0]) and min(a[1], b[1]) <= u.tile[1] <= max(a[1], b[1]):
                    if u.tile not in (a, b):
                        exp.deck_seen.add(identity)
                beyond = u and sum((v-s)*(t-s) for v,s,t in zip(u.tile,a,b)) >= sum((t-s)**2 for s,t in zip(a,b))
                if beyond and identity in exp.deck_seen and dist(u.tile, b) <= 3:
                    exp.crossed = 'bridge'
                    exp.proof = dict(kind='bridge', unit=identity, tick=ctx.tick, banks=exp.bridge)
                    board.bump('bridge_crossings')

    def focus(self, ctx, tile):
        if tile is not None and ctx.board.cooling('expedition-view', ctx.tick):
            return
        ctx.require_order_budget()
        ctx.client.focus_view(tile)
        ctx.orders += 1
        ctx.board.cool('expedition-view', ctx.tick, 8 * ctx.world.tick_hz)

    def step(self, ctx):
        exp, cfg, board = ctx.board.expedition, ctx.config, ctx.board
        if exp is None or board.cooling('expedition-step', ctx.tick):
            return
        board.cool('expedition-step', ctx.tick, cfg.projection_interval_seconds * ctx.world.tick_hz)
        soldiers = [u for i in exp.crew if (u := ctx.world.unit(i)) is not None and u.alive]
        worker = ctx.world.unit(exp.builder)
        if worker and not worker.alive:
            worker = None
        if ctx.tick - exp.started > cfg.projection_timeout_seconds * ctx.world.tick_hz:
            self.finish(ctx)
            return
        if exp.phase in ('board', 'sail'):
            ferry(self, ctx, exp, soldiers, worker)
            return
        if not soldiers:
            if worker and exp.phase in ('outpost', 'attack'):
                self.focus(ctx, worker.tile)
                self.outpost(ctx, exp, soldiers, worker)
            else:
                self.finish(ctx)
            return
        lead = min(soldiers, key=lambda u: dist(u.tile, exp.goal))
        self.focus(ctx, lead.tile)
        if exp.phase == 'stage':
            length = max(1, dist(exp.origin, exp.goal))
            stage = clamp_tile(tuple(round(a + (b-a) * cfg.projection_stage_distance / length)
                                     for a,b in zip(exp.origin, exp.goal)), ctx.world.map_size)
            stage = ctx.unrefused(ctx.land_frontier(exp.origin, stage) or exp.origin)
            if sum(dist(u.tile, stage) <= 6 for u in soldiers) < min(len(soldiers), cfg.projection_stage_min):
                ctx.order('formation_move', [u.id for u in soldiers], *stage, Formation.BOX)
                return
            exp.phase = 'advance'
        if exp.phase == 'advance':
            known = [b for b in ctx.world.known_enemy_buildings() if b.id not in board.cleared
                     and ctx.settlement_distance(b.center) >= cfg.projection_distance]
            if known:
                target = min(known, key=lambda b: (b.kind in (BuildingKind.CASTLE, BuildingKind.TOWN_CENTER), dist(lead.tile,b.center),b.id))
                exp.target, exp.goal = target.id, target.center
            elif not exp.target and dist(lead.tile, exp.goal) <= cfg.projection_stage_distance:
                width, height = ctx.world.map_size
                probes = [(width*x//4, height*y//4) for x,y in
                          ((1,1),(3,1),(3,3),(1,3),(2,1),(3,2),(2,3),(1,2))]
                exp.goal = probes[exp.probe % len(probes)]
                exp.probe += 1
            if exp.target and dist(lead.tile, exp.goal) <= cfg.projection_forward_radius * 2:
                exp.phase = 'outpost'
            else:
                route = ctx.land_route(lead.tile, exp.goal)
                frontier = route[-1] if route else (ctx.land_frontier(lead.tile, exp.goal) or lead.tile)
                if frontier is None:
                    return
                if not route and cfg.build_bridges and worker:
                    site = ctx.world.find_bridge_site(*frontier, *exp.goal)
                    if site and not any(b.kind == BuildingKind.BRIDGE and not b.complete for b in ctx.buildings):
                        a,b = site
                        price = tuple(n * (max(abs(a[0]-b[0]),abs(a[1]-b[1]))-1) for n in building_cost(BuildingKind.BRIDGE))
                        if ctx.land_route(worker.tile,a) is not None and ctx.try_spend(price):
                            ctx.order('build_bridge',[worker.id],*a,*b)
                            exp.bridge = site
                            board.plans.append(Plan(BuildingKind.BRIDGE,(min(a[0],b[0]),min(a[1],b[1])),ctx.tick))
                            board.builders[worker.id] = (str(BuildingKind.BRIDGE),a)
                            board.bump('bridges_ordered')
                            return
                if not route and dist(lead.tile, frontier) <= 8 and any(
                    ctx.world.water_passable(frontier[0]+dx,frontier[1]+dy) is True
                    for dx,dy in ((1,0),(-1,0),(0,1),(0,-1))):
                    if any(b.kind == BuildingKind.BRIDGE and not b.complete and dist(b.center, frontier)<16
                           for b in ctx.buildings):
                        return
                    exp.phase = 'board'
                    ferry(self,ctx,exp,soldiers,worker)
                    return
                hop = ctx.land_hop(lead.tile, frontier)
                if hop != lead.tile:
                    ctx.order('formation_move',[u.id for u in soldiers],*hop,Formation.STAGGERED,attack_move=True)
                    if worker and dist(worker.tile,lead.tile)>6:
                        ctx.order('move',worker.id,*lead.tile)
                return
        if exp.phase in ('outpost','attack'):
            self.outpost(ctx, exp, soldiers, worker)

    def outpost(self, ctx, exp, soldiers, worker):
        cfg = ctx.config
        target = ctx.world.building(exp.target) if exp.target else None
        if target is None:
            self.remember(ctx, exp)
            self.finish(ctx)
            return
        near = min(soldiers,key=lambda u:dist(u.tile,exp.goal)).tile if soldiers else worker.tile
        if worker and dist(worker.tile, near)>8:
            hop = ctx.land_hop(worker.tile,near)
            if hop != worker.tile:
                ctx.order('move',worker.id,*hop)
            return
        for kind in (BuildingKind.BARRACKS,BuildingKind.SIEGE_WORKSHOP):
            present = [b for b in ctx.buildings if b.kind == kind and dist(b.center,exp.goal)<=cfg.projection_forward_radius*2]
            if (not present and kind in exp.sites
                    and not any(p.kind == kind and p.tile == exp.sites[kind] for p in ctx.board.plans)):
                exp.sites.pop(kind)
            if not present and worker and kind not in exp.sites:
                if self.constructor.place(ctx,kind,near,[worker],radius=8,gap=1):
                    exp.sites[kind] = ctx.board.plans[-1].tile
                return
            if present and not all(b.complete for b in present):
                return
        exp.phase = 'attack'
        producers = [b for b in ctx.buildings if b.complete and b.kind in (BuildingKind.BARRACKS, BuildingKind.SIEGE_WORKSHOP)
                     and dist(b.center,exp.goal)<=cfg.projection_forward_radius*2]
        for b in producers[:2]:
            if b.queue_length or b.researching is not None:
                continue
            kind = UnitKind.BATTERING_RAM if b.kind==BuildingKind.SIEGE_WORKSHOP else UnitKind.SPEARMAN
            if kind==UnitKind.SPEARMAN:
                for tech, upgrade in ((Tech.HALBERDIER,UnitKind.HALBERDIER),(Tech.PIKEMAN,UnitKind.PIKEMAN)):
                    if ctx.economy.has_researched(tech):
                        kind=upgrade
                        break
            if ctx.try_spend(unit_cost(kind)):
                ctx.order('train',b.id,kind)
                ctx.queued += 1
        if len(soldiers)>=cfg.projection_attack_min or any(u.kind in (UnitKind.BATTERING_RAM,UnitKind.CAPPED_RAM,UnitKind.SIEGE_RAM) for u in soldiers):
            for u in soldiers:
                if ctx.orders>=ctx.order_limit:
                    break
                if u.idle and u.attack_target is None:
                    ctx.order('attack',u.id,target.id)

    def observe(self, ctx):
        exp = ctx.board.expedition
        for event in ctx.events:
            if exp and event.kind == 'unit_spawned' and event.mine and exp.phase in ('outpost', 'attack'):
                unit = ctx.world.unit(event.id)
                if (unit and unit.alive and unit.kind in MILITARY and unit.commanded_by is None
                        and unit.id not in exp.crew and len(exp.crew) < ctx.config.projection_army_size
                        and dist(unit.tile, exp.goal) < ctx.config.projection_forward_radius * 2):
                    exp.crew.append(unit.id)
            if exp and event.kind == 'unit_died' and event.id in exp.crew:
                self.remember(ctx, exp)
                exp.crew.remove(event.id)
            if event.kind == 'building_completed':
                b=ctx.world.building(event.id)
                if b and b.kind == BuildingKind.BRIDGE:
                    ctx.board.bump('bridges_built')
                if exp and b and exp.sites.get(b.kind)==(b.x,b.y):
                    ctx.board.bump('forward_buildings_built')
            if exp and event.kind=='building_destroyed' and not event.mine and event.id==exp.target:
                self.remember(ctx, exp)

    def remember(self, ctx, exp):
        ctx.board.projection_records.append(dict(target=exp.target,crew=list(exp.crew),
            origin=exp.origin,goal=exp.goal,crossing=exp.crossed,proof=dict(exp.proof),tick=ctx.tick))
        ctx.board.projection_records=ctx.board.projection_records[-32:]

    def finish(self, ctx):
        self.focus(ctx,None)
        ctx.board.expedition=None
        ctx.board.cool('expedition',ctx.tick,60*ctx.world.tick_hz)
