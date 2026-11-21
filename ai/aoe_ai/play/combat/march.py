"""Bounded formation routing shared by the field army and staging orders."""

from bisect import bisect_right

from aoeworld import Formation

from ...geometry import centroid, dist
from ...worker_view import focus_army
from ..scouting import mission_destination


def march(ctx, army, middle, goal, *, purpose="attack"):
    board, cfg = ctx.board, ctx.config
    hz = ctx.world.tick_hz
    pause = cfg.march_pause_seconds * hz
    side = cfg.march_group_tiles
    recalling = purpose == "recall"
    cells = {}
    for soldier in army:
        last = board.marched.get(soldier.id, -pause)
        replace = recalling and (last < board.raid_started
                                 or (soldier.attack_target is not None and ctx.tick - last >= pause))
        if soldier.id in ctx.busy or not (replace or (soldier.idle and ctx.tick - last >= pause)):
            continue
        x, y = soldier.tile
        independent = (purpose == "attack"
                       and board.cooling(f"march-alone:{soldier.id}", ctx.tick))
        cells.setdefault((x // side, y // side, soldier.id if independent else -1), []).append(soldier)

    keys = sorted(cells)
    after = board.march_after.get(purpose)
    start = bisect_right(keys, after) if after is not None else 0
    for cell in keys[start:] + keys[:start]:
        if ctx.orders >= ctx.order_limit or (purpose != "attack" and ctx.land_budget_exhausted()):
            return
        board.march_after[purpose] = cell
        soldiers = cells[cell]
        origin = centroid([u.tile for u in soldiers]) or middle
        if not ctx.world.land_known(*origin):
            origin = next((u.tile for u in soldiers if ctx.world.land_known(*u.tile)), origin)
        target = ctx.land_hop(origin, goal)
        if target == origin and purpose == "attack" and (
                not ctx.world.land_known(*origin) or not ctx.world.land_known(*goal)):
            target = mission_destination(ctx, goal) or origin
        if target == origin and purpose != "rally":
            reachable = ctx.land_frontier(origin, goal)
            if reachable is not None:
                target = ctx.land_hop(origin, reachable)
        if target == origin and recalling:
            # A player can order a short move toward their own raided base
            # when their terrain view is elsewhere. The server checks the hop.
            reach = min(1.0, cfg.march_hop_tiles / max(1.0, dist(origin, goal)))
            target = ctx.unrefused(tuple(round(a + (b - a) * reach) for a, b in zip(origin, goal)))
        if target == origin:
            if purpose != "rally" and not ctx.world.land_known(*origin):
                focus_army(ctx, origin)
            continue
        limit = cfg.march_group_members
        soldiers.sort(key=lambda u: (board.marched.get(u.id, -pause), u.id))
        for offset in range(0, len(soldiers), limit):
            if ctx.orders >= ctx.order_limit:
                return
            members = soldiers[offset:offset + limit]
            ids = [u.id for u in members]
            if len(ids) == 1 and not recalling:
                ctx.order("move", ids[0], *target)
            else:
                ctx.order("formation_move", ids, *target, getattr(Formation, cfg.army_formation),
                          attack_move=purpose == "attack")
            for unit in members:
                board.marched[unit.id] = ctx.tick
                board.march_progress[unit.id] = (unit.tile, ctx.tick)
            if recalling:
                board.bump("defenders_recalled", len(members))
