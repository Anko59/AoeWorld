"""Refresh the public view over large idle settlements away from the army."""
from collections import Counter

from .context import Context
from .play.economy import unemployed


def focus_army(ctx: Context, tile) -> None:
    board, cfg = ctx.board, ctx.config
    key = "army-view"
    if board.army_view_until or board.cooling(key, ctx.tick):
        return
    hz = ctx.world.tick_hz
    ctx.client.focus_view(tile)
    board.army_view_until = ctx.tick + cfg.worker_view_seconds * hz
    board.worker_view_until = 0
    board.cool(key, ctx.tick, cfg.worker_view_interval_seconds * hz)


def survey_workers(ctx: Context) -> None:
    board, cfg = ctx.board, ctx.config
    if board.army_view_until:
        if ctx.tick >= board.army_view_until:
            ctx.client.focus_view()
            board.army_view_until = 0
        return
    if not ctx.late_economy and not board.worker_view_until:
        return
    threat = (board.threat_until > ctx.tick and board.threat_tile is not None
              and ctx.settlement_distance(board.threat_tile) <= cfg.defend_radius)
    if board.worker_view_until:
        if ctx.tick >= board.worker_view_until or threat:
            ctx.client.focus_view()
            board.worker_view_until = 0
        return
    if not ctx.late_economy or threat or board.cooling("worker-view", ctx.tick):
        return
    if len(ctx.villagers) < cfg.worker_view_idle_minimum:
        return
    side = cfg.worker_view_cell_tiles
    missing: Counter[tuple[int, int]] = Counter()
    for worker in ctx.villagers:
        if not unemployed(worker) or worker.id in ctx.busy or worker.id in ctx.reserved_workers:
            continue
        x, y = worker.tile
        if not ctx.world.land_known(x, y):
            missing[x // side, y // side] += 1
    hz = ctx.world.tick_hz
    board.cool("worker-view", ctx.tick, cfg.worker_view_interval_seconds * hz)
    for (x, y), count in sorted(missing.items(), key=lambda pair: (-pair[1], pair[0])):
        if count < cfg.worker_view_idle_minimum:
            break
        cell = f"worker-view:{x}:{y}"
        if board.cooling(cell, ctx.tick):
            continue
        ctx.client.focus_view((x * side + side // 2, y * side + side // 2))
        board.worker_view_until = ctx.tick + cfg.worker_view_seconds * hz
        board.cool(cell, ctx.tick, cfg.worker_view_revisit_seconds * hz)
        break
