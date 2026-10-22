"""Training the army: a spearman, archer and skirmisher mix from finished
production buildings, parked at a rally point near home."""

from __future__ import annotations

from typing import Any, Optional

from aoeworld import BuildingKind, UnitKind, trainable, unit_cost

from .context import Context
from .geometry import Tile, clamp_tile, dist

TRAINERS = (BuildingKind.BARRACKS, BuildingKind.ARCHERY_RANGE)
#: What stands in for a kind the age does not allow yet.
SUBSTITUTE = {UnitKind.SPEARMAN: UnitKind.MILITIA}
MIX_KINDS = {
    "SPEARMAN": UnitKind.SPEARMAN,
    "ARCHER": UnitKind.ARCHER,
    "SKIRMISHER": UnitKind.SKIRMISHER,
}


class MilitaryManager:
    def step(self, ctx: Context) -> None:
        if ctx.economy is None or ctx.home is None:
            return
        self.train(ctx)
        self.rally(ctx)

    # ----- training --------------------------------------------------------
    def composition(self, ctx: Context) -> dict[UnitKind, int]:
        have = {kind: 0 for kind in MIX_KINDS.values()}
        for unit in ctx.army:
            # Militia stand in for spearmen until the age allows them.
            kind = UnitKind.SPEARMAN if unit.kind == UnitKind.MILITIA else unit.kind
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
        if len(ctx.villagers) < ctx.config.army_min_villagers:
            return
        mix = ctx.config.army_mix
        have = self.composition(ctx)
        for building in ctx.buildings:
            if building.kind not in TRAINERS or not building.complete:
                continue
            if building.queue_length >= 2 or building.researching is not None:
                continue
            if ctx.free_population() <= 0:
                return
            choice = self.pick(ctx, building, have, mix)
            if choice is None:
                continue
            ctx.spend(unit_cost(choice))
            ctx.order("train", building.id, choice)
            ctx.queued += 1
            have[UnitKind.SPEARMAN if choice == UnitKind.MILITIA else choice] += 1
            ctx.memory.bump("army_queued")

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
        blocked = ctx.memory.blocked_units
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
        return clamp_tile(point, ctx.world.map_size)

    def rally(self, ctx: Context) -> None:
        if ctx.memory.attacking:
            return
        point = self.rally_point(ctx)
        sent = 0
        for unit in ctx.army:
            if unit.idle and dist(unit.tile, point) > 5 and sent < 12:
                ctx.order("move", unit.id, point[0], point[1])
                sent += 1
