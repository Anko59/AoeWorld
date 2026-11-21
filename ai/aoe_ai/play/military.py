"""Compose an army from completed production buildings and rally it."""

from __future__ import annotations

from typing import Optional

from aoeworld import Building, UnitKind

from . import buildorder
from .combat.march import march
from ..roster import (best_recruit, land_producers, land_soldier, options, price,
                      profile, visible_composition)
from ..context import Context
from ..geometry import Tile, clamp_tile, dist, nearest

TRAINERS = land_producers()


class MilitaryManager:
    def train(self, ctx: Context) -> None:
        if ctx.board.cooling("world-limit:train", ctx.tick):
            return
        defending = buildorder.emergency_defence(ctx)
        minimum = ctx.config.emergency_army_villagers if defending else ctx.config.army_min_villagers
        if ctx.home is None or not ctx.feudal or len(ctx.villagers) < minimum:
            return
        # Castle Age planning reserves specific resources for development.
        # The Feudal opening can still pause the whole army for its age-up.
        if ctx.hold_military_training or (ctx.saving_castle and (not ctx.castle or ctx.center is None)):
            return
        if (not ctx.castle and len(ctx.villagers) >= ctx.config.castle_villagers
                and buildorder.castle_requirement_kinds(ctx) < 2 and not defending):
            return
        trainers = [b for b in ctx.buildings if b.kind in TRAINERS and b.complete]
        if not trainers:
            return
        start = ctx.board.military_cursor % len(trainers)
        researched = tuple(ctx.economy.researched)
        mix = {getattr(UnitKind, name, None): weight for name, weight in ctx.config.army_mix.items()}
        mix[UnitKind.KNIGHT] = ctx.config.castle_knight_weight
        have = ctx.combat_lines.copy()
        enemies = visible_composition(ctx.enemies, lambda kind: not profile(kind).naval)
        known = [b for b in ctx.world.known_enemy_buildings() if not ctx.board.cooling(f"target:{b.id}", ctx.tick)]
        target: Optional[Building] = next((b for b in known if b.id == ctx.board.attack_target), None)
        if target is None:
            target = nearest(known, ctx.home, key=lambda b: b.center)
        target_kind = target.kind if target else None
        for offset in range(len(trainers)):
            if ctx.orders >= ctx.order_limit:
                return
            index = (start + offset) % len(trainers)
            building = trainers[index]
            ctx.board.military_cursor = index + 1
            if (building.queue_length >= ctx.config.army_training_queue
                    or building.researching is not None or building.id in ctx.draining):
                continue
            if ctx.free_population() <= 0:
                return
            roster = options(building.kind, ctx.economy.age, researched)
            choices = [kind for kind in roster if land_soldier(kind)]
            if not ctx.castle:
                # The Feudal opening spends its early income on its core lines.
                choices = [kind for kind in choices if profile(kind).line in mix]
                if str(UnitKind.SPEARMAN) in ctx.board.blocked_units and UnitKind.SPEARMAN in choices:
                    choices += [kind for kind in roster if profile(kind).line == UnitKind.MILITIA]
            kind = best_recruit(ctx, choices, have, enemies, target_kind, mix)
            if (kind is None and not ctx.castle and choices
                    and all(price(unit)[1] > ctx.economy.wood for unit in choices)):
                # The Barracks can still train from food and gold while timber recovers.
                infantry = [unit for unit in roster if profile(unit).line == UnitKind.MILITIA]
                kind = best_recruit(ctx, infantry, have, enemies, target_kind, mix)
            if kind is None:
                continue
            ctx.spend(price(kind))
            ctx.order("train", building.id, kind)
            ctx.queued += 1
            have[profile(kind).line] += 1
            ctx.board.bump("army_queued")

    # ----- rally -----------------------------------------------------------
    def rally_point(self, ctx: Context, home: Optional[Tile] = None) -> Tile:
        home = home or ctx.home
        # Between the Town Center and the map centre: toward where trouble comes from.
        width, height = ctx.world.map_size
        cx, cy = width // 2, height // 2
        offset = ctx.config.rally_offset
        d = max(1.0, dist(home, (cx, cy)))
        point = (
            home[0] + int(round((cx - home[0]) / d * offset)),
            home[1] + int(round((cy - home[1]) / d * offset)),
        )
        # Walk the point off anything the server refused as a destination.
        bad = ctx.board.bad_moves
        for dx, dy in ((0, 0), (2, 0), (0, 2), (-2, 0), (0, -2), (3, 3), (-3, 3), (3, -3), (-3, -3), (5, 0), (0, 5), (-5, 0), (0, -5)):
            candidate = clamp_tile((point[0] + dx, point[1] + dy), ctx.world.map_size)
            if candidate not in bad:
                return candidate
        return clamp_tile(point, ctx.world.map_size)

    def rally_production(self, ctx: Context) -> None:
        """Set one changed server-owned rally per decision, cycling over training sites."""
        trainers = [b for b in ctx.buildings if b.complete and b.kind in TRAINERS]
        if not trainers:
            return
        cfg, board = ctx.config, ctx.board
        for _ in range(min(cfg.rally_scan_trainers, len(trainers))):
            i = board.military_rally_cursor % len(trainers)
            board.military_rally_cursor = i + 1
            trainer = trainers[i]
            key = f"military-rally:{trainer.id}"
            if board.cooling(key, ctx.tick):
                continue
            home = nearest(ctx.centers, trainer.center, key=lambda tile: tile) or ctx.home
            point = self.rally_point(ctx, home)
            if dist(trainer.center, point) > cfg.march_hop_tiles:
                point = self.rally_point(ctx, trainer.center)
            if trainer.rally_position == point:
                continue
            if ctx.land_route(trainer.center, point) is None:
                return
            ctx.order("set_rally", trainer.id, *point)
            board.cool(key, ctx.tick, cfg.rally_retry_seconds * ctx.world.tick_hz)
            return

    def rally(self, ctx: Context) -> None:
        if ctx.home is None:
            return
        self.rally_production(ctx)
        if ctx.board.raid_until > ctx.tick:
            return
        point = self.rally_point(ctx)
        field = ctx.board.attack_members
        if ctx.board.attacking and not field:
            return
        troops = [u for u in ctx.army if u.alive and u.id not in field and dist(u.tile, point) > 5]
        march(ctx, troops, ctx.home, point, purpose="rally")
