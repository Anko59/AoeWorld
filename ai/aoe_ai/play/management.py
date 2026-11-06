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

    # ----- buildings -------------------------------------------------------
    def expand(self, ctx: Context) -> None:
        """A second (third...) Town Center on the way to the map centre."""
        cfg, board = ctx.config, ctx.board
        total = ctx.count(BuildingKind.TOWN_CENTER) + sum(1 for p in board.plans if p.kind == BuildingKind.TOWN_CENTER)
        if (total >= cfg.town_centers or len(ctx.villagers) < cfg.expand_villagers or not ctx.castle
                or board.cooling("expand", ctx.tick)):
            return
        cost = building_cost(BuildingKind.TOWN_CENTER)
        if not ctx.can_afford(cost):
            # Castle Age is bought: keep the wood back for the Town Center, shares lean to wood.
            have = (ctx.economy.food, ctx.economy.wood, ctx.economy.gold, ctx.economy.stone)
            ctx.spent[1] += max(0, min(cost[1], have[1] - ctx.spent[1]))
            board.bump("saving_for_town_center")
            return
        width, height = ctx.world.map_size
        d = max(1.0, dist(ctx.home, (width // 2, height // 2)))
        toward = math.atan2(height // 2 - ctx.home[1], width // 2 - ctx.home[0])
        ring = cfg.expand_distance * (1 + total // 6)
        tried = 0
        for turn in (0, 1, -1, 2, -2, 3):
            angle = toward + turn * math.pi / 3 + (total % 6) * 0.2
            near = clamp_tile((round(ctx.home[0] + math.cos(angle) * ring), round(ctx.home[1] + math.sin(angle) * ring)),
                              ctx.world.map_size)
            if near in board.bad_moves or ctx.land_route(ctx.home, near) is None:
                continue
            builders = self.builders(ctx, near, 4)
            tried += 1
            if builders and self.place(ctx, BuildingKind.TOWN_CENTER, near, builders, 10):
                board.bump("expansions")
                board.cool("expand", ctx.tick, 40 * ctx.world.tick_hz)
                return
            if tried >= 3:
                break
        board.cool("expand", ctx.tick, 20 * ctx.world.tick_hz)

    def castles(self, ctx: Context) -> None:
        cfg, board = ctx.config, ctx.board
        if not cfg.castles or not ctx.castle or board.cooling("castle", ctx.tick):
            return
        total = ctx.count(BuildingKind.CASTLE) + sum(1 for p in board.plans if p.kind == BuildingKind.CASTLE)
        if total >= cfg.castles:
            return
        cost = building_cost(BuildingKind.CASTLE)
        if not ctx.can_afford(cost):
            return
        builders = self.builders(ctx, ctx.home, 5)
        if builders and self.place(ctx, BuildingKind.CASTLE, ctx.home, builders, 24, 2):
            board.bump("castles_placed")
        board.cool("castle", ctx.tick, 10 * ctx.world.tick_hz)

    def builders(self, ctx: Context, tile: Tile, count: int) -> list:
        free = sorted((v for v in ctx.villagers if v.alive and v.id not in ctx.busy
                       and v.task not in (UnitTask.BUILDING, UnitTask.SEEDING, UnitTask.FARMING)
                       and v.id not in ctx.board.farmers.values()),
                      key=lambda v: dist(v.tile, tile))
        return free[:count]

    def place(self, ctx: Context, kind: BuildingKind, near: Tile, builders: list, radius: int, gap: int = 1) -> bool:
        from ..blackboard import Plan

        if ctx.board.cooling("world-limit:build", ctx.tick) or not ctx.try_spend(building_cost(kind)):
            return False
        site = None
        for reach in (radius, 2 * radius):
            for extra in (0, 1, 2):
                found = ctx.world.find_build_site(kind, near[0], near[1], reach, gap + extra)
                if found is not None and (str(kind), *found) not in ctx.board.bad_sites:
                    site = found
                    break
            if site is not None:
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
        if not ctx.castle:
            return
        # Stewards: one per Town Center, from enough villagers.
        if cfg.stewards and len(ctx.villagers) >= cfg.steward_min_villagers:
            have = self.fielded(ctx, UnitKind.STEWARD) + self.in_production(ctx, UnitKind.STEWARD)
            if have < min(cfg.stewards, ctx.count(BuildingKind.TOWN_CENTER, complete=True)):
                self.train(ctx, BuildingKind.TOWN_CENTER, UnitKind.STEWARD,
                           lambda b: not garrisoned(b, UnitKind.STEWARD))
        castles_ok = ctx.count(BuildingKind.CASTLE, complete=True) > 0
        if not castles_ok:
            return
        vassals = sum(1 for u in ctx.units if u.kind == UnitKind.VASSAL) + sum(
            1 for b in ctx.buildings if b.kind == BuildingKind.CASTLE and garrisoned(b, UnitKind.VASSAL))
        queued = sum(1 for b in ctx.buildings if b.kind == BuildingKind.CASTLE and b.queue_length)
        if vassals + queued < min(cfg.vassals, ctx.count(BuildingKind.CASTLE, complete=True)):
            self.train(ctx, BuildingKind.CASTLE, UnitKind.VASSAL, lambda b: not garrisoned(b, UnitKind.VASSAL))
        elif cfg.generals and sum(1 for u in ctx.units if u.kind == UnitKind.GENERAL) + queued < cfg.generals:
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
                board.producing.setdefault(str(kind), []).append((ctx.tick, self.fielded(ctx, kind)))
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

    # ----- Generals ----------------------------------------------------------
    def generals(self, ctx: Context) -> None:
        cfg, board = ctx.config, ctx.board
        for general in (u for u in ctx.units if u.kind == UnitKind.GENERAL and u.alive):
            if general.id not in board.generals:
                board.generals.add(general.id)
                if cfg.general_train:
                    ctx.order("general_train", general.id, True)
                ctx.order("general_mix", general.id, list(cfg.general_mix))
            elif (cfg.general_train and general.army_size is not None and general.army_size < cfg.general_refill_below
                  and not board.cooling(f"gtrain:{general.id}", ctx.tick)):
                # A full army switches the Train button off for good: press it again after losses.
                ctx.order("general_train", general.id, True)
                board.cool(f"gtrain:{general.id}", ctx.tick, 30 * ctx.world.tick_hz)
                board.bump("general_refills")
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
