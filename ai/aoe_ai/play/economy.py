"""Villager production and the balance between food, wood, gold and stone."""

from __future__ import annotations

import heapq
from typing import Any, Optional

from aoeworld import (
    AnimalKind,
    BuildingKind,
    ResourceKind,
    Tech,
    tech_cost,
    UnitKind,
    UnitTask,
    unit_cost,
)

from . import buildorder
from .constructor import Constructor
from .naval import food_reachable
from .reseeding import queue_reseeds
from ..context import Context
from ..geometry import clamp_tile, dist, dist2, nearest, ring_points

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
#: Most idle villagers dispatched in one decision (spreads the order burst).
MAX_ASSIGN_PER_DECISION = 16
#: Seconds before a villager that fell idle again is given another job.
ASSIGN_PAUSE_SECONDS = 4
#: Hunt orders on one animal before it is written off.
HUNT_TRIES = 12
#: Seconds a hunted animal may stay unchanged before its hunters are stood down.
HUNT_STALL_SECONDS = 40


def unemployed(villager: Any) -> bool:
    """Failed jobs and completed builders can leave a worker holding a load.

    A live gather job waiting for a drop-off still exposes its resource kind.
    Reassign only orphaned loads, preserving ordinary return/drop-off retries.
    """
    return villager.alive and (villager.idle or (
        villager.task == UnitTask.CARRYING and not villager.moving
        and villager.gathering is None
    ))


class EconomyManager:
    def __init__(self, constructor: Constructor) -> None:
        self.constructor = constructor

    def produce(self, ctx: Context) -> None:
        """Queue villagers (before houses are placed, so the margin sees them)."""
        if ctx.economy is not None and ctx.home is not None:
            self.train_villagers(ctx)

    def work(self, ctx: Context) -> None:
        """Put idle villagers to work and repair imbalanced live jobs."""
        if ctx.economy is None or ctx.home is None:
            return
        self.forget_farmers(ctx)
        self.drop_stuck_hunts(ctx)
        self.castle_food(ctx)
        self.assign_idle(ctx)
        self.rebalance(ctx)
        queue_reseeds(ctx)

    def castle_food(self, ctx: Context) -> None:
        """Keep gold for the age and its first army upgrade before moving miners."""
        food, _, gold, _ = tech_cost(Tech.CASTLE_AGE)
        if not buildorder.wants_castle_food(ctx) or ctx.economy.food >= food or ctx.economy.gold < gold + tech_cost(Tech.CROSSBOWMAN)[2]:
            return
        pause = ASSIGN_PAUSE_SECONDS * ctx.world.tick_hz
        for worker in ctx.villagers:
            if (worker.gathering == ResourceKind.GOLD and worker.carrying is None
                    and worker.id not in ctx.busy and worker.id not in ctx.board.builders
                    and ctx.tick - ctx.board.assigned.get(worker.id, -pause) >= pause
                    and self.send_to_food(ctx, worker)):
                ctx.board.assigned[worker.id] = ctx.tick
                ctx.board.bump("castle_food_workers")
                return

    # ----- production ------------------------------------------------------
    def train_villagers(self, ctx: Context) -> None:
        center = ctx.center
        if center is None or ctx.hold_town_center or center.researching is not None:
            return
        for kind in (ResourceKind.FOOD, ResourceKind.WOOD):
            candidates = heapq.nsmallest(48, ctx.resources(kind), key=lambda n: dist2(n.tile, ctx.home))
            target = next((n for n in candidates
                           if dist(n.tile, ctx.home) <= ctx.config.max_gather_distance
                           and ctx.land_route(ctx.home, n.tile) is not None), None)
            if target is not None:
                if getattr(center, "rally_resource", None) != target.id:
                    ctx.order("set_rally_resource", center.id, target.id)
                break
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
            ctx.board.bump("villagers_queued")

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
        if not ctx.feudal and len(ctx.villagers) >= cfg.feudal_villagers - 4:
            shares["FOOD"] += 0.15
            shares["WOOD"] -= 0.10
            shares["GOLD"] = max(0.0, shares["GOLD"] - 0.05)
        if buildorder.wants_castle_food(ctx) and ctx.economy.gold >= tech_cost(Tech.CASTLE_AGE)[2] + tech_cost(Tech.CROSSBOWMAN)[2]:
            shares["FOOD"] += shares["GOLD"]
            shares["GOLD"] = 0.0
        # Early on nothing but food and wood matter.
        if len(ctx.villagers) < 10:
            shares.update(GOLD=0.0, STONE=0.0)
        return shares

    def assign_idle(self, ctx: Context) -> None:
        idle = [
            v
            for v in ctx.villagers
            if unemployed(v) and v.id not in ctx.busy and ctx.board.builders.get(v.id) is None
        ]
        # A builder whose foundation finished (or vanished) is a worker again.
        for v in ctx.villagers:
            if v.id in ctx.board.builders and unemployed(v) and v.id not in ctx.busy:
                del ctx.board.builders[v.id]
                idle.append(v)
        if not idle:
            return
        counts = self.counts(ctx)
        shares = self.shares(ctx)
        total = max(1, len(ctx.villagers))
        pause = ASSIGN_PAUSE_SECONDS * ctx.world.tick_hz
        assigned = ctx.board.assigned
        alive = {v.id for v in ctx.villagers}
        for gone in [i for i in assigned if i not in alive]:
            del assigned[gone]
        self.learn_unreachable(ctx, idle)
        given = 0
        for villager in idle:
            # A villager that goes idle right after an order cannot do it:
            # give it a pause instead of repeating the order every decision.
            if ctx.tick - assigned.get(villager.id, -pause) < pause:
                continue
            if given >= MAX_ASSIGN_PER_DECISION:
                break  # the rest wait for the next decision, unstamped
            assigned[villager.id] = ctx.tick
            given += 1
            order = sorted(KEYS, key=lambda k: shares[k] * total - counts[k], reverse=True)
            for key in order:
                if shares[key] <= 0 and key != order[0]:
                    continue
                if self.send(ctx, villager, key):
                    counts[key] += 1
                    break
            else:
                self.explore(ctx, villager)

    def rebalance(self, ctx: Context) -> None:
        """Rallied and reassigned workers need not fall idle to change resources.

        Move at most one empty-handed worker, leaving a one-worker margin so
        rounding and the next spawn cannot bounce workers between jobs.
        """
        counts, shares = self.counts(ctx), self.shares(ctx)
        total = max(1, len(ctx.villagers))
        gaps = {key: shares[key] * total - counts[key] for key in KEYS}
        target = max(gaps, key=gaps.get)
        source = min(gaps, key=gaps.get)
        if gaps[target] <= 1 or gaps[source] >= -1:
            return
        pause = ASSIGN_PAUSE_SECONDS * ctx.world.tick_hz
        for worker in ctx.villagers:
            if (worker.gathering != KEYS[source] or worker.carrying is not None
                    or worker.task in (UnitTask.HUNTING, UnitTask.FARMING, UnitTask.BUILDING)
                    or worker.id in ctx.busy or worker.id in ctx.board.builders
                    or worker.id in ctx.board.farmers.values()
                    or ctx.tick - ctx.board.assigned.get(worker.id, -pause) < pause):
                continue
            if self.send(ctx, worker, target):
                ctx.board.assigned[worker.id] = ctx.tick
                ctx.board.bump("workers_rebalanced")
                return

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
        self.gather(ctx, villager, node)
        return True

    def gather(self, ctx: Context, villager: Any, node: Any) -> None:
        ctx.order("gather", villager.id, node.id)
        ctx.board.gather_sent[villager.id] = (node.id, ctx.tick)

    def learn_unreachable(self, ctx: Context, idle: list) -> None:
        """A villager idle right after a gather order could not reach the node:
        leave that node alone for a while instead of sending villagers there
        again (each such trip costs the server a hopeless route search)."""
        sent = ctx.board.gather_sent
        alive = {v.id for v in ctx.villagers}
        for gone in [i for i in sent if i not in alive]:
            del sent[gone]
        window = ctx.config.gather_fail_seconds * ctx.world.tick_hz
        settle = 2 * ctx.world.tick_hz  # the order itself may still be in flight
        for villager in idle:
            last = sent.get(villager.id)
            if last is None or ctx.tick - last[1] < settle:
                continue
            del sent[villager.id]
            if ctx.tick - last[1] > window:
                continue
            ctx.board.cool(
                f"unreachable:{last[0]}:villager",
                ctx.tick,
                ctx.config.gather_unreachable_seconds * ctx.world.tick_hz,
            )
            ctx.board.bump("gather_unreachable")

    def explore(self, ctx: Context, villager: Any) -> None:
        """Nothing known to work: walk a ring around home to uncover something."""
        mem = ctx.board
        mem.bump("explored")
        ring = 12 + 6 * (mem.stats["explored"] // 8)
        points = ring_points(ctx.home, min(ring, 60), spacing=8)
        goal = clamp_tile(points[mem.stats["explored"] % len(points)], ctx.world.map_size)
        target = goal if ctx.land_route(villager.tile, goal) else ctx.land_frontier(villager.tile, goal)
        if target is not None:
            target = ctx.land_hop(villager.tile, target)
            if target != villager.tile:
                ctx.order("move", villager.id, *target)

    def best_node(self, ctx: Context, villager: Any, kind: ResourceKind) -> Optional[Any]:
        home = ctx.home
        reach = ctx.config.max_gather_distance
        scored = []
        # The nearest few dozen to the villager are plenty to choose from.
        candidates = heapq.nsmallest(
            48, ctx.resources(kind), key=lambda n: dist2(n.tile, villager.tile)
        )
        for node in candidates:
            if dist(node.tile, home) > reach or ctx.board.cooling(
                f"unreachable:{node.id}:villager", ctx.tick
            ):
                continue
            drop = min(ctx.dropoff_distance(node), 60.0)
            score = 2.0 * drop + dist(node.tile, villager.tile)
            scored.append((score, node.id, node))
        # Check in preference order, leaving the bounded query budget available
        # for other workers once a connected resource has been found.
        return next((node for _, _, node in sorted(scored)
                     if food_reachable(ctx, villager, node)), None)

    # ----- food ------------------------------------------------------------
    def send_to_food(self, ctx: Context, villager: Any) -> bool:
        """Sheep first, then berries (with a Mill beside them), then deer and
        boar, then farms once the land runs dry."""
        sheep = self.huntable(ctx, (AnimalKind.SHEEP,))
        if sheep is not None:
            return self.hunt(ctx, villager, sheep)
        node = self.best_node(ctx, villager, ResourceKind.FOOD)
        if node is not None:
            if ctx.dropoff_distance(node) > ctx.config.dropoff_distance:
                if self.constructor.ensure_camp(ctx, BuildingKind.MILL, node, villager):
                    return True
            self.gather(ctx, villager, node)
            return True
        game = self.huntable(ctx, (AnimalKind.DEER, AnimalKind.BOAR))
        if game is not None:
            return self.hunt(ctx, villager, game)
        return self.send_to_farm(ctx, villager)

    def hunt(self, ctx: Context, villager: Any, animal: Any) -> bool:
        """Hunt order that gives up on an animal nobody manages to finish."""
        tries = ctx.board.hunt_tries
        tries[animal.id] = tries.get(animal.id, 0) + 1
        if tries[animal.id] > HUNT_TRIES:
            ctx.board.bad_resources.add(animal.id)
            ctx.board.bump("hunts_abandoned")
        ctx.order("hunt", [villager.id], animal.id)
        ctx.board.hunters[villager.id] = animal.id
        return True

    def drop_stuck_hunts(self, ctx: Context) -> None:
        """Hunters never fall idle, so a quarry they cannot reach or catch would
        hold them for ever: when neither its health nor its meat has changed
        for a while, write it off and stand the hunters down for a new job."""
        mem = ctx.board
        hunters = {v.id: v for v in ctx.villagers if v.task == UnitTask.HUNTING}
        for gone in [i for i in mem.hunters if i not in hunters]:
            del mem.hunters[gone]
        animals = {a.id: a for a in ctx.world.animals()}
        limit = HUNT_STALL_SECONDS * ctx.world.tick_hz
        for animal_id in set(mem.hunters.values()):
            animal = animals.get(animal_id)
            if animal is None:
                continue
            state = (animal.hp, animal.food_remaining)
            seen = mem.hunt_watch.get(animal_id)
            if seen is None or seen[0] != state:
                mem.hunt_watch[animal_id] = (state, ctx.tick)
            elif ctx.tick - seen[1] > limit:
                mem.bad_resources.add(animal_id)
                mem.bump("hunts_abandoned")
                for villager_id in [v for v, a in mem.hunters.items() if a == animal_id]:
                    ctx.order("stop", villager_id)
                    del mem.hunters[villager_id]
                    mem.assigned.pop(villager_id, None)

    def huntable(self, ctx: Context, kinds: tuple) -> Optional[Any]:
        for kind in kinds:
            animals = [
                a
                for a in ctx.world.animals()
                if a.kind == kind
                and a.owner in (None, ctx.world.me)
                and a.food_remaining > 0
                and dist(a.tile, ctx.home) <= ctx.config.hunt_distance
                and a.id not in ctx.board.bad_resources
            ]
            found = nearest(animals, ctx.home)
            if found is not None:
                return found
        return None

    def forget_farmers(self, ctx: Context) -> None:
        workers = {v.id: v for v in ctx.villagers}
        farms = {b.id: b for b in ctx.buildings if b.kind == BuildingKind.FARM}
        farmers = ctx.board.farmers
        pause = ASSIGN_PAUSE_SECONDS * ctx.world.tick_hz
        for farm, worker in list(farmers.items()):
            unit = workers.get(worker)
            if unit is None or farm not in farms or (
                unit.gathering is None and unit.task != UnitTask.FARMING
                and ctx.tick - ctx.board.assigned.get(worker, -pause) >= pause
            ):
                del farmers[farm]
        # Construction automatically puts a farm's builder to work. That job
        # did not originate in send_to_farm, but its visible work pose and tile
        # identify the occupied plot, including after a reconnect.
        for worker in ctx.villagers:
            if worker.task != UnitTask.FARMING:
                continue
            x, y = worker.tile
            for farm in farms.values():
                if farm.x <= x < farm.x + farm.width and farm.y <= y < farm.y + farm.height:
                    farmers[farm.id] = worker.id
                    break

    def send_to_farm(self, ctx: Context, villager: Any) -> bool:
        farmers = ctx.board.farmers
        free = [
            f
            for f in ctx.world.my_buildings(BuildingKind.FARM, complete_only=True)
            if f.id not in farmers
            and not ctx.board.cooling(f"farm:{f.id}", ctx.tick)
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
