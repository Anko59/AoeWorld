"""Villager production and the balance between food, wood, gold and stone."""

from __future__ import annotations

import heapq
from typing import Any, Optional

from aoeworld import (
    AnimalKind,
    BuildingKind,
    ResourceKind,
    UnitKind,
    UnitTask,
    unit_cost,
)

from .constructor import Constructor
from .context import Context
from .geometry import clamp_tile, dist, dist2, nearest, ring_points

KEYS = {
    "FOOD": ResourceKind.FOOD,
    "WOOD": ResourceKind.WOOD,
    "GOLD": ResourceKind.GOLD,
    "STONE": ResourceKind.STONE,
}
CAMPS = {
    ResourceKind.WOOD: BuildingKind.LUMBER_CAMP,
    ResourceKind.GOLD: BuildingKind.MINING_CAMP,
    ResourceKind.STONE: BuildingKind.MINING_CAMP,
    ResourceKind.FOOD: BuildingKind.MILL,
}
HUNTABLE = (AnimalKind.SHEEP, AnimalKind.DEER, AnimalKind.BOAR)
#: Most idle villagers dispatched in one decision (spreads the order burst).
MAX_ASSIGN_PER_DECISION = 16


class EconomyManager:
    def __init__(self, constructor: Constructor) -> None:
        self.constructor = constructor

    def produce(self, ctx: Context) -> None:
        """Queue villagers (before houses are placed, so the margin sees them)."""
        if ctx.economy is not None and ctx.home is not None:
            self.train_villagers(ctx)

    def work(self, ctx: Context) -> None:
        """Put idle villagers to work."""
        if ctx.economy is None or ctx.home is None:
            return
        self.forget_farmers(ctx)
        self.assign_idle(ctx)

    # ----- production ------------------------------------------------------
    def train_villagers(self, ctx: Context) -> None:
        center = ctx.center
        if center is None or ctx.hold_town_center or center.researching is not None:
            return
        queued_villagers = center.queue_length
        if len(ctx.villagers) + queued_villagers >= ctx.config.max_villagers:
            return
        queue = center.queue_length
        cost = unit_cost(UnitKind.VILLAGER)
        while (
            queue < ctx.config.town_center_queue
            and ctx.free_population() > 0
            and ctx.try_spend(cost)
        ):
            ctx.order("train", center.id, UnitKind.VILLAGER)
            ctx.queued += 1
            queue += 1
            ctx.memory.bump("villagers_queued")

    # ----- allocation ------------------------------------------------------
    def counts(self, ctx: Context) -> dict[str, int]:
        counts = {key: 0 for key in KEYS}
        for v in ctx.villagers:
            if v.task in (UnitTask.HUNTING, UnitTask.FARMING):
                counts["FOOD"] += 1
            elif v.gathering is not None:
                for key, kind in KEYS.items():
                    if v.gathering == kind:
                        counts[key] += 1
        return counts

    def shares(self, ctx: Context) -> dict[str, float]:
        cfg = ctx.config
        shares = dict(cfg.feudal_shares if ctx.feudal else cfg.dark_shares)
        # Saving 500 food for Feudal Age: lean on food.
        if not ctx.feudal and len(ctx.villagers) >= cfg.feudal_min_villagers:
            shares["FOOD"] += 0.15
            shares["WOOD"] -= 0.10
            shares["STONE"] = max(0.0, shares["STONE"] - 0.05)
        # Early on nothing but food and wood matter.
        if len(ctx.villagers) < 10:
            shares.update(GOLD=0.0, STONE=0.0)
        return shares

    def assign_idle(self, ctx: Context) -> None:
        idle = [
            v
            for v in ctx.villagers
            if v.idle and v.id not in ctx.busy and ctx.memory.builders.get(v.id) is None
        ]
        # A builder whose foundation finished (or vanished) is a worker again.
        for v in ctx.villagers:
            if v.id in ctx.memory.builders and v.idle and v.id not in ctx.busy:
                del ctx.memory.builders[v.id]
                idle.append(v)
        if not idle:
            return
        counts = self.counts(ctx)
        shares = self.shares(ctx)
        total = max(1, len(ctx.villagers))
        for villager in idle[:MAX_ASSIGN_PER_DECISION]:
            order = sorted(KEYS, key=lambda k: shares[k] * total - counts[k], reverse=True)
            for key in order:
                if shares[key] <= 0 and key != order[0]:
                    continue
                if self.send(ctx, villager, key):
                    counts[key] += 1
                    break
            else:
                self.explore(ctx, villager)

    def send(self, ctx: Context, villager: Any, key: str) -> bool:
        if key == "FOOD":
            return self.send_to_food(ctx, villager)
        kind = KEYS[key]
        node = self.best_node(ctx, villager, kind)
        if node is None:
            return False
        if ctx.dropoff_distance(node) > ctx.config.dropoff_distance:
            if self.constructor.ensure_camp(ctx, CAMPS[kind], node, villager):
                return True
        ctx.order("gather", villager.id, node.id)
        return True

    def explore(self, ctx: Context, villager: Any) -> None:
        """Nothing known to work: walk a ring around home to uncover something."""
        mem = ctx.memory
        mem.bump("explored")
        ring = 12 + 6 * (mem.stats["explored"] // 8)
        points = ring_points(ctx.home, min(ring, 60), spacing=8)
        x, y = clamp_tile(points[mem.stats["explored"] % len(points)], ctx.world.map_size)
        ctx.order("move", villager.id, x, y)

    def best_node(self, ctx: Context, villager: Any, kind: ResourceKind) -> Optional[Any]:
        home = ctx.home
        reach = ctx.config.max_gather_distance
        best, best_score = None, None
        # The nearest few dozen to the villager are plenty to choose from.
        candidates = heapq.nsmallest(
            48, ctx.resources(kind), key=lambda n: dist2(n.tile, villager.tile)
        )
        for node in candidates:
            if dist(node.tile, home) > reach:
                continue
            drop = min(ctx.dropoff_distance(node), 60.0)
            score = 2.0 * drop + dist(node.tile, villager.tile)
            if best_score is None or score < best_score:
                best, best_score = node, score
        return best

    # ----- food ------------------------------------------------------------
    def send_to_food(self, ctx: Context, villager: Any) -> bool:
        home = ctx.home
        hunt = self.huntable(ctx)
        if hunt is not None:
            ctx.order("hunt", [villager.id], hunt.id)
            return True
        node = self.best_node(ctx, villager, ResourceKind.FOOD)
        if node is not None:
            if ctx.dropoff_distance(node) > ctx.config.dropoff_distance:
                if self.constructor.ensure_camp(ctx, BuildingKind.MILL, node, villager):
                    return True
            ctx.order("gather", villager.id, node.id)
            return True
        return self.send_to_farm(ctx, villager)

    def huntable(self, ctx: Context) -> Optional[Any]:
        animals = [
            a
            for a in ctx.world.animals()
            if a.kind in HUNTABLE
            and a.owner is None
            and a.food_remaining > 0
            and dist(a.tile, ctx.home) <= ctx.config.hunt_distance
        ]
        return nearest(animals, ctx.home)

    def forget_farmers(self, ctx: Context) -> None:
        alive = {v.id for v in ctx.villagers}
        farms = {b.id for b in ctx.buildings}
        farmers = ctx.memory.farmers
        for farm in [f for f, v in farmers.items() if v not in alive or f not in farms]:
            del farmers[farm]

    def send_to_farm(self, ctx: Context, villager: Any) -> bool:
        farmers = ctx.memory.farmers
        free = [
            f
            for f in ctx.world.my_buildings(BuildingKind.FARM, complete_only=True)
            if f.id not in farmers
        ]
        farm = nearest(free, villager.tile, key=lambda b: b.center)
        if farm is not None:
            farmers[farm.id] = villager.id
            ctx.order("farm", villager.id, farm.id)
            return True
        mill = nearest(
            [b for b in ctx.buildings if b.kind == BuildingKind.MILL and b.complete],
            ctx.home,
            key=lambda b: b.center,
        )
        if mill is None:
            if self.constructor.total(ctx, BuildingKind.MILL) == 0:
                return self.constructor.place(
                    ctx, BuildingKind.MILL, ctx.home, [villager], radius=10, gap=1
                )
            return False
        if self.constructor.pending(ctx, BuildingKind.FARM) >= 2:
            return False
        farms = self.constructor.total(ctx, BuildingKind.FARM)
        if farms >= ctx.config.farm_target + len(ctx.villagers) // 8:
            return False
        return self.constructor.place(ctx, BuildingKind.FARM, mill.center, [villager], radius=9, gap=0)
