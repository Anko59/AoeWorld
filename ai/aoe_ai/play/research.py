"""Age advances and research from the public technology tree."""

from __future__ import annotations

from collections import defaultdict

from aoeworld import Age, BuildingKind, Tech, UnitKind

from ..context import Context
from ..roster import AGE_RANK, land_producers, profile, research_price, technology_rows
from . import buildorder
from .management import funding_reserve

AGE_TECHS = frozenset((Tech.LOOM, Tech.FEUDAL_AGE, Tech.CASTLE_AGE, Tech.IMPERIAL_AGE))
ARMS_RESEARCH = frozenset((BuildingKind.BLACKSMITH, BuildingKind.UNIVERSITY))


class ResearchManager:
    def step(self, ctx: Context) -> None:
        eco = ctx.economy
        center = ctx.center
        if eco is None or center is None:
            return
        stage = buildorder.stage(ctx)
        if stage == buildorder.IMPERIAL_ADVANCE:
            # Everything for the age stands: hold villagers and soldiers until
            # it is bought, then resume while it researches.
            if center.researching is None:
                ctx.saving_castle = True
                ctx.hold_military_training = True
                ctx.hold_town_center = True
                if not self.research(ctx, Tech.IMPERIAL_AGE, center):
                    ctx.reserve(research_price(Tech.IMPERIAL_AGE))
                return
        if stage == buildorder.CASTLE_ADVANCE:
            ctx.saving_castle = True
            ctx.hold_town_center = True
            self.research(ctx, Tech.CASTLE_AGE, center)
            return
        if buildorder.wants_castle_food(ctx):
            # The Feudal army may keep growing, but villagers and optional
            # upgrades must not consume the age's food reserve meanwhile.
            ctx.hold_town_center = True
            if ctx.config.castle_first:
                ctx.saving_castle = True  # no soldiers until the age is bought
            return
        if stage == buildorder.ADVANCE:
            # Everything for the age is standing: stop spending on anything but
            # the age and let food pile up to its price.
            if not self.research(ctx, Tech.FEUDAL_AGE, center):
                ctx.hold_town_center = True
        elif not eco.has_researched(Tech.LOOM) and buildorder.wants_loom(ctx):
            self.research(ctx, Tech.LOOM, center)
        if ctx.feudal:
            self.upgrades(ctx)

    def research(self, ctx: Context, tech: Tech, building) -> bool:
        key = "research:" + str(tech)
        if any(b.researching == tech for b in ctx.buildings) or building.researching is not None or ctx.board.cooling(key, ctx.tick):
            return False
        cost = research_price(tech)
        if ctx.can_afford(cost):
            if building.queue_length == 0:
                ctx.spend(cost)
                ctx.order("research", building.id, tech)
                ctx.draining.add(building.id)
                ctx.board.cool(key, ctx.tick, 3 * ctx.world.tick_hz)
                return True
            # Affordable but the queue is busy: stop feeding it so it drains.
            if building.kind == BuildingKind.TOWN_CENTER:
                ctx.hold_town_center = True
        return False

    def upgrades(self, ctx: Context) -> None:
        researched = set(ctx.economy.researched)
        active = {b.researching for b in ctx.buildings if b.researching is not None}
        pending = [order for order in ctx.board.tracked.values() if order.name == "research"]
        active.update(order.args[1] for order in pending)
        pending_buildings = {order.args[0] for order in pending}
        sites = defaultdict(list)
        complete = set()
        researching_kinds = set()
        for building in ctx.buildings:
            if building.complete:
                complete.add(building.kind)
                if building.researching is None and building.id not in pending_buildings:
                    sites[building.kind].append(building)
                else:
                    researching_kinds.add(building.kind)
        candidates = [row for row in technology_rows()
                      if row.tech not in AGE_TECHS and row.tech not in researched and row.tech not in active
                      and row.building in sites and AGE_RANK[row.age] <= AGE_RANK[ctx.economy.age]
                      and (row.prerequisite is None or row.prerequisite in researched)
                      and len(complete.intersection(row.building_kinds)) >= row.building_count
                      and not ctx.board.cooling("research:" + str(row.tech), ctx.tick)]
        if not candidates:
            return
        fielded = ctx.combat_lines
        castle_opening = ctx.castle and not ctx.imperial and ctx.board.stats.get("castle_units_peak", 0) == 0
        if castle_opening and self.first_castle_upgrade(ctx, candidates, active | researched, sites, fielded):
            return

        reserve = funding_reserve(ctx)
        food_float = ctx.config.optional_research_food_float
        wood_float = ctx.config.optional_research_wood_float + buildorder.castle_prerequisite_wood(ctx)
        gold_goal = 0
        military_producers = land_producers() | {BuildingKind.DOCK}
        ranked = [(self.priority(row, fielded), row) for row in candidates]
        for priority, row in sorted(ranked, key=lambda entry: entry[0]):
            available = sites[row.building]
            if not available or (row.building in military_producers and row.building in researching_kinds):
                continue
            if (row.building == BuildingKind.TOWN_CENTER and ctx.castle
                    and len(ctx.villagers) < ctx.config.imperial_villagers and len(available) == 1):
                continue
            if not ctx.imperial and row.building in land_producers():
                affects_army = (any(fielded[line] for line in row.upgrades)
                                or any(fielded[profile(unit).line] for unit in row.improves))
                if castle_opening or not affects_army:
                    continue
            cost = research_price(row.tech)
            arms = row.upgrades or row.unlocks or row.improves or row.building in ARMS_RESEARCH
            # Fielded lines and new unlocks earn their actual price first in
            # Imperial. The first management units still precede optional tech.
            needs_management_budget = not (ctx.imperial and priority[0] <= 1)
            needed = tuple(a + b for a, b in zip(cost, reserve)) if needs_management_budget else cost
            if not arms or not ctx.castle:
                needed = (needed[0] + food_float, needed[1] + wood_float, *needed[2:])
            if not ctx.can_afford(needed):
                if arms and ctx.imperial and gold_goal == 0 and ctx.can_afford((needed[0], needed[1], 0, needed[3])):
                    # Hold the gold now, so cheaper rows later in this pass
                    # cannot spend what the higher-priority upgrade waits for.
                    gold_goal = cost[2]
                    ctx.spent[2] += min(max(0, ctx.economy.gold - ctx.spent[2]), gold_goal)
                continue
            building = min(available, key=lambda b: (b.queue_length, b.id))
            self.claim(ctx, row, building)
            if not ctx.imperial:
                return
            if row.building in military_producers or not building.idle:
                available.clear()  # keep sibling military producers training
            else:
                available.remove(building)

    def first_castle_upgrade(self, ctx, candidates, started, sites, fielded) -> bool:
        """Fund the opening Castle upgrade before spending the next food delivery."""
        if any(row.age == Age.CASTLE and row.upgrades and row.tech in started for row in technology_rows()):
            return False
        upgrades = [row for row in candidates if row.age == Age.CASTLE
                    and row.upgrades and row.building in land_producers()]
        row = next((row for row in upgrades if any(fielded[line] for line in row.upgrades)), None)
        if row is None and len(ctx.army) + len(ctx.led) >= ctx.config.castle_army_min:
            core = {getattr(UnitKind, name, None) for name in ctx.config.army_mix}
            row = next((row for row in upgrades if core.intersection(row.upgrades)), None)
        if row is None:
            return False
        building = min(sites[row.building], key=lambda b: (b.queue_length, b.id))
        cost = research_price(row.tech)
        ctx.hold_town_center = True
        if building.idle and ctx.can_afford(cost):
            self.claim(ctx, row, building)
        else:
            ctx.saving_castle = True
            ctx.hold_military_training = True
            ctx.reserve(cost)
        return True

    def claim(self, ctx, row, building) -> None:
        ctx.spend(research_price(row.tech))
        ctx.draining.add(building.id)
        if building.idle:
            ctx.order("research", building.id, row.tech)
            ctx.board.cool("research:" + str(row.tech), ctx.tick,
                           ctx.config.research_retry_seconds * ctx.world.tick_hz)
        elif building.kind in land_producers():
            ctx.production_drain = True

    @staticmethod
    def priority(row, fielded) -> tuple:
        beneficiaries = sum(fielded[line] for line in row.upgrades)
        age = -AGE_RANK[row.age]
        if beneficiaries:
            return 0, -beneficiaries, age
        if any(profile(unit).line == unit for unit in row.unlocks):
            return 1, 0, age
        if row.upgrades or row.economic or row.building in ARMS_RESEARCH:
            return 2, 0, age
        return 3, 0, age
