"""Recall the field force after damage to a settlement or worker."""

from ...geometry import dist, nearest, near_any
from .march import march


def observe(ctx):
    board, cfg = ctx.board, ctx.config
    hz = ctx.world.tick_hz
    hits = [event for event in ctx.events if event.kind == "under_attack"]
    if not hits:
        return
    board.bump("threats_seen", len(hits))
    board.threat_until = ctx.tick + cfg.threat_seconds * hz
    board.threat_tile = hits[-1].x, hits[-1].y

    workers = {u.id for u in ctx.villagers}
    attackers = {u.id: u for u in ctx.enemies}
    wildlife = {a.id for a in ctx.world.animals()}
    for event in reversed(hits):
        attacker = getattr(event, "attacker", None)
        enemy = attackers.get(attacker)
        is_building = getattr(event, "is_building", False)
        if not is_building and (getattr(event, "id", None) not in workers
                                or attacker is None or attacker in wildlife):
            continue
        tile = event.x, event.y
        if board.raid_tile is None or board.raid_until <= ctx.tick or dist(board.raid_tile, tile) > cfg.defend_radius:
            board.raid_started, board.raid_tile = ctx.tick, tile
            board.attacking = False
            board.attack_members.clear()
            board.attack_target = board.march_goal = None
            board.search_index = 0
            board.counter_owner = None
            board.counter_bearing = None
        board.raid_until = ctx.tick + cfg.raid_hold_seconds * hz
        if enemy is not None:
            board.counter_owner = enemy.owner
            if board.counter_bearing is None:
                board.counter_bearing = enemy.tile[0] - ctx.home[0], enemy.tile[1] - ctx.home[1]
            board.counter_until = board.raid_until + cfg.counter_seconds * hz
        return


def respond(ctx, army, engage, pull_back):
    board, cfg = ctx.board, ctx.config
    if board.raid_until <= ctx.tick:
        return False
    at = board.raid_tile
    # Stay near the hit while raiders remain visible around the settlement.
    raiders = [e for e in ctx.enemies if e.alive and dist(e.tile, at) <= cfg.defend_radius]
    if raiders:
        board.raid_until = ctx.tick + cfg.raid_hold_seconds * ctx.world.tick_hz
        if any(e.owner == board.counter_owner for e in raiders):
            board.counter_until = board.raid_until + cfg.counter_seconds * ctx.world.tick_hz
    if len(raiders) >= 3 and len(army) * 2 < len(raiders):
        shelter = min(ctx.centers, key=lambda center: dist(center, at), default=ctx.home)
        pull_back(ctx, raiders, shelter=shelter, limit=4)
    local = [u for u in army if dist(u.tile, at) <= cfg.defend_radius]
    engage(ctx, local, raiders)
    goal = nearest(raiders, at).tile if raiders else at
    contact = cfg.combat_engage_tiles if raiders else cfg.defend_radius // 2
    nearby = near_any((e.tile for e in raiders), cfg.combat_engage_tiles)
    # Troops already fighting the raiders stay in that fight; distant troops
    # may abandon an unrelated march or attack on the first recall hop.
    remote = [u for u in army if dist(u.tile, goal) > contact
              and (not nearby(u.tile) or u.attack_target is None)]
    march(ctx, remote, ctx.home, goal, purpose="recall")
    board.bump("raid_defences")
    return True
