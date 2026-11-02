"""Training the army: a spearman, archer and skirmisher mix from finished
production buildings, parked at a rally point near home."""

from __future__ import annotations

from typing import Any, Optional

from aoeworld import BuildingKind, Tech, UnitKind, trainable, unit_cost
from . import buildorder

from ..context import Context
from ..geometry import Tile, clamp_tile, dist

TRAINERS = (BuildingKind.BARRACKS, BuildingKind.ARCHERY_RANGE, BuildingKind.STABLE)
#: What stands in for a kind the age does not allow yet.
SUBSTITUTE = {UnitKind.SPEARMAN: UnitKind.MILITIA}
MIX_KINDS = {
    "SPEARMAN": UnitKind.SPEARMAN,
    "ARCHER": UnitKind.ARCHER,
    "SKIRMISHER": UnitKind.SKIRMISHER,
    "KNIGHT": UnitKind.KNIGHT,
}


class MilitaryManager:
    # ----- training --------------------------------------------------------
    def composition(self, ctx: Context) -> dict[UnitKind, int]:
        have = {kind: 0 for kind in MIX_KINDS.values()}
        for unit in ctx.army:
            # Militia stand in for spearmen until the age allows them.
            kind = UnitKind.SPEARMAN if unit.kind == UnitKind.MILITIA else unit.kind
            kind = {UnitKind.CROSSBOWMAN: UnitKind.ARCHER, UnitKind.PIKEMAN: UnitKind.SPEARMAN}.get(kind, kind)
            if kind in have:
                have[kind] += 1
        for building in ctx.buildings:
            if building.kind in TRAINERS:
                have_queue = building.queue_length
                # Queue contents are not itemised per kind in the summary, so
                # attribute them to what the building makes first.
                options = [k for k in trainable(building.kind) if k in have]
                if options and have_queue:
                    have[options[0]] += have_queue
        return have

    def train(self, ctx: Context) -> None:
        if ctx.home is None or ctx.saving_castle or not ctx.feudal or len(ctx.villagers) < ctx.config.army_min_villagers:
            return
        # Let wood accumulate for the second distinct Feudal building before
        # production consumes every 25/35-wood increment.
        if (not ctx.castle and len(ctx.villagers) >= ctx.config.castle_villagers
                and buildorder.castle_requirement_kinds(ctx) < 2):
            return
        mix = dict(ctx.config.army_mix)
        mix["KNIGHT"] = ctx.config.castle_knight_weight
        have = self.composition(ctx)
        for building in ctx.buildings:
            if building.kind not in TRAINERS or not building.complete:
                continue
            if building.queue_length >= 2 or building.researching is not None or building.kind in ctx.draining:
                continue
            choice = self.pick(ctx, building, have, mix)
            if choice is None:
                continue
            ctx.spend(unit_cost(choice))
            ctx.order("train", building.id, choice)
            bucket = {UnitKind.MILITIA: UnitKind.SPEARMAN, UnitKind.PIKEMAN: UnitKind.SPEARMAN, UnitKind.CROSSBOWMAN: UnitKind.ARCHER}.get(choice, choice)
            have[bucket] += 1
            ctx.board.bump("army_queued")

    def pick(self, ctx: Context, building: Any, have: dict, mix: dict) -> Optional[UnitKind]:
        options = []
        for kind in trainable(building.kind):
            name = next((n for n, k in MIX_KINDS.items() if k == kind), None)
            if name is None:
                continue
            train = self.allowed(ctx, kind)
            if train is None or not ctx.can_afford(unit_cost(train)):
                continue
            options.append((have[kind] / mix.get(name, 1.0), train))
        if not options:
            return None
        options.sort(key=lambda pair: pair[0])
        return options[0][1]

    def allowed(self, ctx: Context, kind: UnitKind) -> Optional[UnitKind]:
        """`kind`, or a stand-in while the server has refused it for the age."""
        if kind == UnitKind.KNIGHT and not ctx.castle:
            return None
        if ctx.castle:
            for base, tech, upgraded in [(UnitKind.ARCHER, Tech.CROSSBOWMAN, UnitKind.CROSSBOWMAN),
                                          (UnitKind.SPEARMAN, Tech.PIKEMAN, UnitKind.PIKEMAN)]:
                if kind == base and ctx.economy.has_researched(tech):
                    return upgraded
        blocked = ctx.board.blocked_units
        if str(kind) not in blocked:
            return kind
        substitute = SUBSTITUTE.get(kind)
        if substitute is not None and str(substitute) not in blocked:
            return substitute
        return None

    # ----- rally -----------------------------------------------------------
    def rally_point(self, ctx: Context) -> Tile:
        home = ctx.home
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

    def rally(self, ctx: Context) -> None:
        if ctx.home is None or ctx.board.attacking:
            return
        point = self.rally_point(ctx)
        sent = 0
        marched = ctx.board.marched
        pause = ctx.config.march_pause_seconds * ctx.world.tick_hz
        for unit in ctx.army:
            if (unit.idle and unit.id not in ctx.busy and dist(unit.tile, point) > 5 and sent < 12
                    and ctx.tick - marched.get(unit.id, -pause) >= pause):
                target = ctx.land_hop(unit.tile, point)
                if target == unit.tile:
                    continue
                marched[unit.id] = ctx.tick
                ctx.order("move", unit.id, *target)
                sent += 1
