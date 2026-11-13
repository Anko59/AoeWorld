"""Local defensive spending from public enemies and recent damage/losses."""
from ...geometry import dist


def observe(ctx):
    board, cfg = ctx.board, ctx.config
    board.defence_alerts = {tile: until for tile, until in board.defence_alerts.items()
                            if until > ctx.tick}
    for event in ctx.events:
        tile = None
        if event.kind == 'under_attack' or (event.kind == 'unit_died' and event.mine):
            x, y = getattr(event, 'x', None), getattr(event, 'y', None)
            if x is not None and y is not None:
                tile = (x, y)
        elif event.kind == 'building_destroyed' and event.mine:
            tile = board.defence_buildings.get(event.id)
        if tile is not None:
            # One remembered alert per local cell; distant army losses cannot
            # authorize home defences. Fixed cap bounds long-game memory.
            cell = tuple(n // cfg.fort_alert_cell for n in tile)
            if cell not in board.defence_alerts and len(board.defence_alerts) >= cfg.fort_alert_limit:
                oldest = min(board.defence_alerts, key=board.defence_alerts.get)
                del board.defence_alerts[oldest]
            board.defence_alerts[cell] = ctx.tick + cfg.fort_threat_seconds * ctx.world.tick_hz
    board.defence_buildings = {b.id: b.center for b in ctx.buildings}


def threatened(ctx, tile):
    """Index once per decision; each settlement probes only adjacent cells.

    No route/site queries or orders. Enemy buildings represent a frontier;
    units and alerts use the smaller local danger radius.
    """
    if not ctx.config.fortifications:
        return False
    if not hasattr(ctx, '_defence_index'):
        cfg = ctx.config
        alerts = [tuple((n + .5) * cfg.fort_alert_cell for n in cell)
                  for cell, until in ctx.board.defence_alerts.items() if until > ctx.tick]
        groups = ((cfg.fort_threat_radius, [u.tile for u in ctx.enemies] + alerts),
                  (cfg.fort_frontier_radius, [b.center for b in ctx.world.known_enemy_buildings()]))
        ctx._defence_index = []
        for radius, points in groups:
            cells = {}
            for point in points:
                cell = tuple(int(n // radius) for n in point)
                cells.setdefault(cell, []).append(point)
            ctx._defence_index.append((radius, cells))
    for radius, cells in ctx._defence_index:
        x, y = (int(n // radius) for n in tile)
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                if any(dist(tile, point) <= radius for point in cells.get((x + dx, y + dy), ())):
                    return True
    return False
