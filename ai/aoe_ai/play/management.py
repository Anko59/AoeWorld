"""Management units and the buildings they need.

* Castles (stone) host Vassals (defence, towers, walls, roads) and Generals.
* A Steward garrisoned in a Town Center manages the villagers of a 100-tile
  domain and builds Houses; one per Town Center.
* A General leads up to 100 soldiers; with his Train button pressed he buys
  them himself from the production buildings in his 50-tile sphere.
* More Town Centers train more villagers and widen the Steward domains.

Prices are saved ahead of army spending (``save_for_management``): a Steward
costs 1,000 food and 1,000 gold, a General 800 and 800.
"""

from __future__ import annotations

from typing import Any, Optional
import math

from aoeworld import BuildingKind, GeneralState, UnitKind, UnitTask, building_cost, train_seconds, unit_cost

from . import buildorder
from ..context import Context
from ..geometry import Tile, clamp_tile, dist


def garrisoned(building: Any, kind: UnitKind) -> bool:
    return any(entry[1] == kind for entry in building.garrison)


def funding_reserve(ctx):
    """Protect the first available manager of each enabled kind from science."""
    cfg = ctx.config
    result = [0, 0, 0, 0]
    if not cfg.save_for_management or not ctx.castle:
        return tuple(result)
    kinds = [(cfg.stewards and len(ctx.villagers) >= cfg.steward_min_villagers,
              UnitKind.STEWARD, BuildingKind.TOWN_CENTER),
             (cfg.vassals, UnitKind.VASSAL, BuildingKind.CASTLE),
             (cfg.generals, UnitKind.GENERAL, BuildingKind.CASTLE)]
    for enabled, kind, house in kinds:
        if not enabled or not ctx.count(house, complete=True):
            continue
        present = any(u.kind == kind for u in ctx.units) or any(garrisoned(b, kind) for b in ctx.buildings)
        pending = any(ctx.tick - tick < (train_seconds(kind) + 20) * ctx.world.tick_hz
                      for tick, _ in ctx.board.producing.get(str(kind), []))
        if not present and not pending:
            for i, price in enumerate(unit_cost(kind)):
                result[i] += price
    return tuple(result)


class ManagementManager:
    # ----- one decision ----------------------------------------------------
    def step(self, ctx: Context) -> None:
        cfg = ctx.config
        if ctx.home is None or ctx.economy is None:
            return
        if not (cfg.castles or cfg.stewards or cfg.vassals or cfg.generals or cfg.town_centers > 1):
            return
        self.expand(ctx)
        self.castles(ctx)
        self.seat(ctx, UnitKind.STEWARD, BuildingKind.TOWN_CENTER)
        self.seat(ctx, UnitKind.VASSAL, BuildingKind.CASTLE)
        self.recruit(ctx)
        self.generals(ctx)
        self.settings(ctx)
        self.defenders(ctx)

    # ----- buildings -------------------------------------------------------
    def expand(self, ctx: Context) -> None:
        """A second (third...) Town Center on the way to the map centre."""
        cfg, board = ctx.config, ctx.board
        total = ctx.count(BuildingKind.TOWN_CENTER) + sum(1 for p in board.plans if p.kind == BuildingKind.TOWN_CENTER)
        if (total >= buildorder.wanted_centers(ctx) or (total > 0 and (len(ctx.villagers) < cfg.expand_villagers or not ctx.castle))
                or board.cooling("expand", ctx.tick)):
            return
        cost = building_cost(BuildingKind.TOWN_CENTER)
        if not ctx.can_afford(cost):
            # Castle Age is bought: keep the wood back for the Town Center, shares lean to wood.
            have = (ctx.economy.food, ctx.economy.wood, ctx.economy.gold, ctx.economy.stone)
            ctx.spent[1] += max(0, min(cost[1], have[1] - ctx.spent[1] - cfg.expansion_wood_float))
            if total == 0 or len(ctx.army) + len(ctx.led) >= cfg.imperial_army_min:
                ctx.saving_castle = True
            board.bump("saving_for_town_center")
            return
        centers = [b.center for b in ctx.buildings if b.kind == BuildingKind.TOWN_CENTER]
        origin = centers[board.expansion_cursor % len(centers)] if centers else ctx.villagers[0].tile
        board.expansion_cursor += 1
        toward = math.atan2(ctx.world.map_size[1] // 2 - origin[1], ctx.world.map_size[0] // 2 - origin[0])
        for turn in range(6):
            angle = toward + (turn + board.expansion_cursor % 6) * math.pi / 3
            near = clamp_tile((round(origin[0] + math.cos(angle) * cfg.settlement_spacing * 1.6),
                               round(origin[1] + math.sin(angle) * cfg.settlement_spacing * 1.6)), ctx.world.map_size)
            if total == 0:
                near = origin
            if any(dist(near, c) < cfg.settlement_spacing for c in centers):
                continue
            if ctx.land_route(origin, near) is None:
                continue
            builders = self.builders(ctx, near, 4)
            if builders and self.place(ctx, BuildingKind.TOWN_CENTER, near, builders, 8):
                board.bump("expansions")
                board.cool("expand", ctx.tick, 40 * ctx.world.tick_hz)
                return
        board.cool("expand", ctx.tick, 20 * ctx.world.tick_hz)

    def castles(self, ctx: Context) -> None:
        cfg, board = ctx.config, ctx.board
        if not cfg.castles or not ctx.castle or board.cooling("castle", ctx.tick):
            return
        total = ctx.count(BuildingKind.CASTLE) + sum(1 for p in board.plans if p.kind == BuildingKind.CASTLE)
        if total >= buildorder.wanted_castles(ctx):
            return
        cost = building_cost(BuildingKind.CASTLE)
        if not ctx.can_afford(cost):
            return
        near = (ctx.centers or [ctx.home])[total % max(1, len(ctx.centers))]
        builders = self.builders(ctx, near, 5)
        if builders and self.place(ctx, BuildingKind.CASTLE, near, builders, 24, 2):
            board.bump("castles_placed")
        board.cool("castle", ctx.tick, 10 * ctx.world.tick_hz)

    def builders(self, ctx: Context, tile: Tile, count: int) -> list:
        free = sorted((v for v in ctx.villagers if v.alive and v.id not in ctx.busy
                       and v.task not in (UnitTask.BUILDING, UnitTask.SEEDING, UnitTask.FARMING)
                       and v.id not in ctx.board.farmers.values() and v.id not in ctx.reserved_workers),
                      key=lambda v: dist(v.tile, tile))
        return free[:count]

    def place(self, ctx: Context, kind: BuildingKind, near: Tile, builders: list, radius: int, gap: int = 1) -> bool:
        from ..blackboard import Plan

        if ctx.board.cooling("world-limit:build", ctx.tick) or not ctx.try_spend(building_cost(kind)):
            return False
        site = None
        for reach, spacing in ((radius, gap), (radius, 0), (2 * radius, gap), (2 * radius, 0)):
            found = ctx.find_build_site(kind, near[0], near[1], reach, spacing)
            if (found is not None and (str(kind), *found) not in ctx.board.bad_sites
                    and (kind != BuildingKind.TOWN_CENTER or all(
                        dist(found, b.center) >= ctx.config.settlement_spacing
                        for b in ctx.buildings if b.kind == kind))):
                site = found
                break
        if site is None:
            cost = building_cost(kind)
            for i in range(4):
                ctx.spent[i] -= cost[i]
            return False
        ctx.order("build", [b.id for b in builders], kind, site[0], site[1])
        ctx.board.plans.append(Plan(kind, site, ctx.tick))
        for b in builders:
            ctx.board.builders[b.id] = (str(kind), site)
        return True

    # ----- Stewards and Vassals ----------------------------------------------
    def seat(self, ctx: Context, kind: UnitKind, house: BuildingKind) -> None:
        """Walk every idle `kind` into a completed `house` that has none."""
        board = ctx.board
        inside = {entry[0] for b in ctx.buildings for entry in b.garrison}
        seats = [b for b in ctx.buildings if b.kind == house and b.complete and not garrisoned(b, kind)]
        taken = {t for u, t in board.seated.items() if any(x.id == u for x in ctx.units)}
        for unit in ctx.units:
            if unit.kind != kind or not unit.alive or unit.id in ctx.busy or unit.id in inside or not unit.idle:
                continue
            target = next((b for b in sorted(seats, key=lambda b: dist(b.center, unit.tile))
                           if b.id not in taken or board.seated.get(unit.id) == b.id), None)
            if target is None or board.cooling(f"seat:{unit.id}", ctx.tick):
                continue
            taken.add(target.id)
            board.seated[unit.id] = target.id
            ctx.order("garrison", unit.id, target.id)
            board.cool(f"seat:{unit.id}", ctx.tick, 15 * ctx.world.tick_hz)
            board.bump(f"seated_{str(kind).split('.')[-1].lower()}")

    def recruit(self, ctx: Context) -> None:
        cfg, board = ctx.config, ctx.board
        if not ctx.castle or (ctx.hold_town_center and ctx.saving_castle):
            return
        # Stewards: one per Town Center, from enough villagers.
        if cfg.stewards and len(ctx.villagers) >= cfg.steward_min_villagers:
            have = self.fielded(ctx, UnitKind.STEWARD) + self.in_production(ctx, UnitKind.STEWARD)
            if have < min(max(cfg.stewards, math.ceil(len(ctx.villagers) / 80)), ctx.count(BuildingKind.TOWN_CENTER, complete=True)):
                self.train(ctx, BuildingKind.TOWN_CENTER, UnitKind.STEWARD,
                           lambda b: not garrisoned(b, UnitKind.STEWARD))
        castles_ok = ctx.count(BuildingKind.CASTLE, complete=True) > 0
        if not castles_ok:
            return
        vassals = sum(1 for u in ctx.units if u.kind == UnitKind.VASSAL) + sum(
            1 for b in ctx.buildings if b.kind == BuildingKind.CASTLE and garrisoned(b, UnitKind.VASSAL))
        queued = sum(1 for b in ctx.buildings if b.kind == BuildingKind.CASTLE and b.queue_length)
        if vassals + queued < (max(cfg.vassals, ctx.count(BuildingKind.CASTLE, complete=True)) if cfg.vassals else 0):
            self.train(ctx, BuildingKind.CASTLE, UnitKind.VASSAL, lambda b: not garrisoned(b, UnitKind.VASSAL))
        elif cfg.generals and sum(1 for u in ctx.units if u.kind == UnitKind.GENERAL) + queued < max(cfg.generals, ctx.count(BuildingKind.CASTLE, complete=True)):
            self.train(ctx, BuildingKind.CASTLE, UnitKind.GENERAL, lambda b: True)

    def fielded(self, ctx: Context, kind: UnitKind) -> int:
        """Units of `kind` that exist: in the field or sheltered in a building."""
        return sum(1 for u in ctx.units if u.kind == kind) + sum(
            1 for b in ctx.buildings for entry in b.garrison if entry[1] == kind and entry[0] not in {u.id for u in ctx.units})

    def in_production(self, ctx: Context, kind: UnitKind) -> int:
        """Units ordered and still training: counted until one more exists or the
        training time (plus a margin for a refused order) has passed."""
        now, board = ctx.tick, ctx.board
        existing = self.fielded(ctx, kind)
        window = (train_seconds(kind) + 20) * ctx.world.tick_hz
        live = [(t, before) for t, before in board.producing.get(str(kind), []) if now - t < window and existing <= before]
        board.producing[str(kind)] = live
        return len(live)

    def train(self, ctx: Context, house: BuildingKind, kind: UnitKind, wants: Any) -> None:
        board = ctx.board
        cost = unit_cost(kind)
        busy = False
        for b in ctx.buildings:
            if b.kind != house or not b.complete or not wants(b):
                continue
            if b.queue_length or b.researching is not None:
                busy = True  # a Town Center queued with villagers: save so the queue drains
                continue
            if board.cooling(f"train:{b.id}", ctx.tick) or ctx.free_population() <= 0:
                continue
            if ctx.try_spend(cost):
                ctx.order("train", b.id, kind)
                ctx.queued += 1
                board.cool(f"train:{b.id}", ctx.tick, 10 * ctx.world.tick_hz)
                # The count this order is waiting to exceed includes the
                # orders still training, so one arrival clears only the oldest.
                pending = self.fielded(ctx, kind) + self.in_production(ctx, kind)
                board.producing.setdefault(str(kind), []).append((ctx.tick, pending))
                board.bump(f"{str(kind).split('.')[-1].lower()}_trained")
                return
            self.save(ctx, kind, cost)
            return
        if busy:
            self.save(ctx, kind, cost)

    def save(self, ctx: Context, kind: UnitKind, cost: tuple) -> None:
        """Save for `kind`: no soldiers, the gold and food shares up. Self-limiting:
        after `save_seconds` the army spends again for `save_pause_seconds`."""
        board, cfg = ctx.board, ctx.config
        key = "save:" + str(kind)
        if not cfg.save_for_management or board.cooling(key, ctx.tick) or buildorder.imperial_ready(ctx):
            return  # the Imperial Age price comes first
        hz = ctx.world.tick_hz
        since = board.save_since.setdefault(key, ctx.tick)
        if ctx.tick - since > cfg.save_seconds * hz:
            del board.save_since[key]
            board.cool(key, ctx.tick, cfg.save_pause_seconds * hz)
            return
        ctx.saving_castle = True
        board.saving_tick = ctx.tick
        board.hold_villagers_tick = ctx.tick  # villagers eat the 1,000 food: pause them meanwhile
        have = (ctx.economy.food, ctx.economy.wood, ctx.economy.gold, ctx.economy.stone)
        for i in range(4):
            ctx.spent[i] += max(0, min(cost[i], have[i] - ctx.spent[i]))
        board.bump("saving_for_management")

    def defenders(self, ctx):
        """Keep a small local workforce available for native Vassal fortification."""
        castles = [b for b in ctx.buildings if b.complete and b.kind == BuildingKind.CASTLE
                   and garrisoned(b, UnitKind.VASSAL)]
        alive = {u.id for u in ctx.villagers}
        board = ctx.board
        board.vassal_workers = {b.id: board.vassal_workers.get(b.id, set()) & alive for b in castles}
        for castle in castles:
            workers = board.vassal_workers[castle.id]
            if len(workers) >= 8:
                continue
            near = (castle.x - 3, castle.y - 3)
            candidates = sorted((v for v in ctx.villagers if v.id not in ctx.busy
                                 and v.id not in ctx.reserved_workers
                                 and v.task not in (UnitTask.BUILDING, UnitTask.FARMING, UnitTask.SEEDING)
                                 and dist(v.tile, castle.center) < 90), key=lambda v: dist(v.tile, castle.center))
            for worker in candidates[:8 - len(workers)]:
                if ctx.land_route(worker.tile, near) is None:
                    continue
                ctx.order("move", worker.id, *near)
                workers.add(worker.id)
                ctx.reserved_workers.add(worker.id)

    def settings(self, ctx):
        """Apply ordinary UI management switches once per change."""
        from .economy import EconomyManager
        shares = EconomyManager(None).shares(ctx)
        weights = tuple(round(shares.get(k, 0) * 100) for k in ("FOOD", "WOOD", "GOLD", "STONE"))
        training = not (ctx.hold_town_center or ctx.board.hold_villagers_tick == ctx.tick)
        for b in ctx.buildings:
            for entry in b.garrison:
                unit, kind = entry[:2]
                if kind == UnitKind.STEWARD:
                    desired = (weights, 100, training)
                    if ctx.board.manager_settings.get(unit) != desired:
                        ctx.order("steward_settings", unit, list(weights), 100, training)
                        ctx.board.manager_settings[unit] = desired
                elif kind == UnitKind.VASSAL:
                    enabled = ctx.count(BuildingKind.CASTLE) >= buildorder.wanted_castles(ctx)
                    desired = (enabled,)
                    if ctx.board.manager_settings.get(unit) != desired:
                        ctx.order("vassal_development", unit, enabled)
                        ctx.board.manager_settings[unit] = desired

    # ----- Generals ----------------------------------------------------------
    def generals(self, ctx: Context) -> None:
        cfg, board = ctx.config, ctx.board
        for general in (u for u in ctx.units if u.kind == UnitKind.GENERAL and u.alive):
            enabled = cfg.general_train and not (ctx.saving_castle or ctx.draining)
            if general.id not in board.generals:
                ctx.order("general_mix", general.id, list(cfg.general_mix))
                board.generals.add(general.id)
            if (board.general_training.get(general.id) != enabled
                    or (enabled and general.army_size is not None and general.army_size < cfg.general_refill_below
                        and not board.cooling(f"gtrain:{general.id}", ctx.tick))):
                ctx.order("general_train", general.id, enabled)
                board.general_training[general.id] = enabled
                board.cool(f"gtrain:{general.id}", ctx.tick, 30 * ctx.world.tick_hz)
        # `army_size` only counts the commanded soldiers; the General seat comes first.
        board.stats["led_peak"] = max(board.stats.get("led_peak", 0), sum(u.army_size or 0 for u in ctx.units if u.kind == UnitKind.GENERAL))

    def lead(self, ctx: Context, combat: Any) -> None:
        """March each General's army at the nearest known enemy building."""
        cfg = ctx.config
        for general in (u for u in ctx.units if u.kind == UnitKind.GENERAL and u.alive):
            if general.id in ctx.busy or general.general_state != GeneralState.IDLE or not general.idle:
                continue
            led = general.army_size or 0
            if led < cfg.general_attack_army:
                if led < cfg.general_retreat_army and dist(general.tile, ctx.home) > 14:
                    ctx.order("move", general.id, *ctx.land_hop(general.tile, ctx.home))
                continue
            building = combat.target_building(ctx, general.tile)
            if building is not None:
                goal = building.center if dist(general.tile, building.center) <= cfg.march_hop_tiles and building.visible \
                    else combat.approach(ctx, building, general.tile)
            else:
                width, height = ctx.world.map_size
                goal = (width - 1 - ctx.home[0], height - 1 - ctx.home[1])
                if dist(general.tile, goal) < 15:
                    goal = (width // 2, height // 2)
            hop = ctx.land_hop(general.tile, goal) if dist(general.tile, goal) > cfg.march_hop_tiles else goal
            if hop == general.tile:
                continue
            ctx.order("move", general.id, *ctx.unrefused(hop))
            ctx.board.bump("general_marches")
