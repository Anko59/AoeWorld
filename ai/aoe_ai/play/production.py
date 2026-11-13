"""Bounded, rotating Town Center production across the living kingdom."""
import heapq

from aoeworld import BuildingKind, ResourceKind, UnitKind, unit_cost

from . import buildorder
from ..geometry import dist2


def train_villagers(ctx):
    if ctx.hold_town_center or ctx.board.cooling('world-limit:train', ctx.tick):
        return
    if ctx.tick - ctx.board.hold_villagers_tick <= 3 * ctx.world.tick_hz:
        return
    centers = [b for b in ctx.buildings if b.kind == BuildingKind.TOWN_CENTER and b.complete]
    if not centers:
        return
    queued = sum(c.queue_length for c in centers)
    target = buildorder.villager_target(ctx)
    if len(ctx.villagers) + queued >= target:
        return
    start = ctx.board.production_cursor % len(centers)
    ctx.board.production_cursor = (start + 1) % len(centers)
    for center in centers[start:] + centers[:start]:
        if center.researching is not None:
            continue
        # Each domain gets its own nearby rally. Managed domains choose jobs
        # themselves; a remote first-TC rally must not drag their workers home.
        if not center.garrison:
            for kind in (ResourceKind.FOOD, ResourceKind.WOOD):
                nodes = heapq.nsmallest(8, ctx.resources(kind), key=lambda n: dist2(n.tile, center.center))
                node = next((n for n in nodes if dist2(n.tile, center.center) <= ctx.config.max_gather_distance ** 2
                             and ctx.land_route(center.center, n.tile) is not None), None)
                if node is not None:
                    if getattr(center, 'rally_resource', None) != node.id:
                        ctx.order('set_rally_resource', center.id, node.id)
                    break
        queue = center.queue_length
        while (queue < ctx.config.town_center_queue and ctx.free_population() > 0
               and len(ctx.villagers) + queued < target and ctx.try_spend(unit_cost(UnitKind.VILLAGER))):
            ctx.order('train', center.id, UnitKind.VILLAGER)
            ctx.queued += 1
            queue += 1
            queued += 1
            ctx.board.bump('villagers_queued')
