"""The build order: which stage the game is in and what it asks for next.

Dark Age: boom (villagers without pause, a Lumber Camp and a Mill),
Loom, then at about twenty villagers save 500 food and click Feudal Age.
Feudal Age: Barracks, Archery Range, Blacksmith, the economy upgrades and a
mixed army. The managers only *execute* this plan.
"""

from __future__ import annotations

from aoeworld import BuildingKind, Tech, building_cost

from ..context import Context

#: At least two different kinds of these must stand before Feudal Age.
FEUDAL_REQUIREMENT = (
    BuildingKind.BARRACKS,
    BuildingKind.MILL,
    BuildingKind.LUMBER_CAMP,
    BuildingKind.MINING_CAMP,
    BuildingKind.DOCK,
)

DARK, ADVANCE, FEUDAL = "dark", "advance", "feudal"
CASTLE_ADVANCE, CASTLE = "castle-advance", "castle"
CASTLE_REQUIREMENT = (BuildingKind.ARCHERY_RANGE, BuildingKind.STABLE, BuildingKind.BLACKSMITH, BuildingKind.MARKET)
IMPERIAL_ADVANCE, IMPERIAL = "imperial-advance", "imperial"
#: At least two different kinds of these must stand before Imperial Age.
IMPERIAL_REQUIREMENT = (BuildingKind.CASTLE, BuildingKind.UNIVERSITY, BuildingKind.MONASTERY, BuildingKind.SIEGE_WORKSHOP)
#: The two the AI raises: cheap wood buildings (the Siege Workshop needs the Blacksmith).
IMPERIAL_BUILDINGS = (BuildingKind.UNIVERSITY, BuildingKind.SIEGE_WORKSHOP)


def requirement_kinds(ctx: Context) -> int:
    """Distinct finished kinds that count toward Feudal Age."""
    return len({b.kind for b in ctx.buildings if b.complete and b.kind in FEUDAL_REQUIREMENT})


def castle_requirement_kinds(ctx: Context) -> int:
    return len({b.kind for b in ctx.buildings if b.complete and b.kind in CASTLE_REQUIREMENT})


def imperial_requirement_kinds(ctx: Context) -> int:
    return len({b.kind for b in ctx.buildings if b.complete and b.kind in IMPERIAL_REQUIREMENT})


def imperial_ready(ctx: Context) -> bool:
    """The Castle Age economy and army the climb to Imperial Age is built on."""
    return (ctx.castle and not ctx.imperial and len(ctx.villagers) >= ctx.config.imperial_villagers
            and ctx.board.stats.get("army_peak", 0) >= ctx.config.imperial_army_min)


def wants_imperial_food(ctx: Context) -> bool:
    """Save the age's food and gold while the Castle army and buildings finish."""
    return imperial_ready(ctx)


def castle_prerequisite_wood(ctx: Context) -> int:
    """Protect the next prerequisite's budget as soon as Feudal begins.

    Paid foundations and pending orders already reserve their own cost. Rich
    stockpiles can still feed farms and soldiers above this bounded reserve.
    """
    if not ctx.feudal or ctx.castle or castle_requirement_kinds(ctx) >= 2:
        return 0
    placed = {b.kind for b in ctx.buildings} | {p.kind for p in ctx.board.plans}
    if len(placed.intersection(CASTLE_REQUIREMENT)) >= 2:
        return 0
    missing = (k for k in (BuildingKind.ARCHERY_RANGE, BuildingKind.BLACKSMITH) if k not in placed)
    return min(building_cost(k)[1] for k in missing) + ctx.config.wood_reserve


def stage(ctx: Context) -> str:
    if ctx.imperial:
        return IMPERIAL
    if ctx.castle:
        if imperial_ready(ctx) and imperial_requirement_kinds(ctx) >= 2:
            return IMPERIAL_ADVANCE
        return CASTLE
    if ctx.feudal:
        ready = castle_requirement_kinds(ctx) >= 2
        return CASTLE_ADVANCE if (ready and len(ctx.villagers) >= ctx.config.castle_villagers
                                  and ctx.board.stats.get("army_peak", 0) >= ctx.config.castle_army_min) else FEUDAL
    if len(ctx.villagers) >= ctx.config.feudal_villagers and requirement_kinds(ctx) >= 2:
        return ADVANCE
    return DARK


def wants_castle_food(ctx: Context) -> bool:
    """Prepare the age's food while the Feudal army finishes forming."""
    return ctx.feudal and not ctx.castle and len(ctx.villagers) >= ctx.config.castle_villagers


def wants_loom(ctx: Context) -> bool:
    return len(ctx.villagers) >= ctx.config.loom_villagers


def unpaid_age_up_barracks(ctx: Context) -> bool:
    """The age-up's first Barracks must precede discretionary wood drop-offs."""
    return (ctx.center is not None and ctx.center.researching == Tech.FEUDAL_AGE
            and not any(b.kind == BuildingKind.BARRACKS for b in ctx.buildings)
            and not any(p.kind == BuildingKind.BARRACKS for p in ctx.board.plans))


def farm_wood_reserve(ctx: Context) -> int:
    """Food expansion must leave wood for houses and the first Barracks.

    Start saving when the villagers are ready to advance, even if food is
    still accumulating. A foundation or pending order has already been paid.
    Free food and existing farms can keep feeding us while wood accumulates.
    """
    placed = any(b.kind == BuildingKind.BARRACKS for b in ctx.buildings) or any(
        p.kind == BuildingKind.BARRACKS for p in ctx.board.plans
    )
    advancing = (ctx.feudal or len(ctx.villagers) >= ctx.config.feudal_villagers
                 or (ctx.center is not None and ctx.center.researching == Tech.FEUDAL_AGE))
    return ctx.config.wood_reserve + (building_cost(BuildingKind.BARRACKS)[1]
                                      if advancing and not placed else 0)


def imperial_prerequisite_wood(ctx):
    if not imperial_ready(ctx) or imperial_requirement_kinds(ctx) >= 2:
        return 0
    placed = {b.kind for b in ctx.buildings} | {p.kind for p in ctx.board.plans}
    if len(placed.intersection(IMPERIAL_REQUIREMENT)) >= 2:
        return 0
    return min((building_cost(k)[1] for k in IMPERIAL_BUILDINGS if k not in placed), default=0) + ctx.config.wood_reserve


def wanted_centers(ctx):
    return max(ctx.config.town_centers, 1 + len(ctx.villagers) // ctx.config.expand_workers_per_center)


def villager_target(ctx):
    return max(ctx.config.base_villagers, ctx.count(BuildingKind.TOWN_CENTER, complete=True) * ctx.config.workers_per_center)


def wanted_castles(ctx):
    return max(ctx.config.castles, (ctx.count(BuildingKind.TOWN_CENTER, complete=True) + 1) // 2) if ctx.config.castles else 0


# ----- buildings ----------------------------------------------------------
def wanted_buildings(ctx: Context) -> list[tuple[BuildingKind, int]]:
    """(kind, how many) in the order they should appear; Feudal Age only.

    Dark Age drop-offs (Lumber Camp, Mill, mining camps) follow the
    resources, see :class:`Constructor`."""
    if not ctx.feudal:
        # The Barracks is a Dark Age building: raise it while the age researches.
        if ctx.center is not None and ctx.center.researching == Tech.FEUDAL_AGE:
            return [(BuildingKind.BARRACKS, 1)]
        return []
    # Finish the Blacksmith before expanding production so the second Castle
    # prerequisite does not compete with another Barracks for construction wood.
    wants = [
        (BuildingKind.BARRACKS, 1),
        (BuildingKind.ARCHERY_RANGE, 1),
    ]
    wants.append((BuildingKind.BLACKSMITH, 1))
    if ctx.castle or (ctx.center is not None and ctx.center.researching == Tech.CASTLE_AGE):
        wants.insert(2, (BuildingKind.STABLE, max(1, ctx.count(BuildingKind.TOWN_CENTER, complete=True) // 2)))
    # The Imperial Age buildings come before the second Barracks and Range:
    # they are the age's prerequisite, the extra production is not.
    if ctx.castle and len(ctx.villagers) >= ctx.config.imperial_villagers - 6:
        wants.extend((kind, 1) for kind in IMPERIAL_BUILDINGS)
    if len(ctx.villagers) >= 22:
        wants.append((BuildingKind.BARRACKS, max(ctx.config.max_barracks, ctx.count(BuildingKind.TOWN_CENTER, complete=True))))
    if len(ctx.villagers) >= 26:
        wants.append((BuildingKind.ARCHERY_RANGE, max(ctx.config.max_ranges, ctx.count(BuildingKind.TOWN_CENTER, complete=True))))
    if len(ctx.villagers) >= 24:
        wants.append((BuildingKind.MARKET, 1))
    return wants


#: A building that must be finished before another can be placed.
PREREQUISITE = {BuildingKind.ARCHERY_RANGE: BuildingKind.BARRACKS, BuildingKind.STABLE: BuildingKind.BARRACKS,
                BuildingKind.SIEGE_WORKSHOP: BuildingKind.BLACKSMITH}


# ----- technologies -------------------------------------------------------
def wanted_upgrades(ctx):
    from aoeworld import Age, technologies
    rank = {Age.DARK: 0, Age.FEUDAL: 1, Age.CASTLE: 2, Age.IMPERIAL: 3}
    ages = (Tech.FEUDAL_AGE, Tech.CASTLE_AGE, Tech.IMPERIAL_AGE)
    all_rules = technologies()
    priority = (Tech.CROSSBOWMAN, Tech.PIKEMAN, Tech.DOUBLE_BIT_AXE, Tech.HORSE_COLLAR)
    if ctx.imperial:
        priority = (Tech.ARBALESTER, Tech.HALBERDIER, Tech.CAVALIER, Tech.BRACER,
                    Tech.BLAST_FURNACE, Tech.PLATE_MAIL_ARMOR, Tech.RING_ARCHER_ARMOR,
                    Tech.PLATE_BARDING_ARMOR, Tech.PALADIN, Tech.CROP_ROTATION,
                    Tech.TWO_MAN_SAW) + priority
    all_rules.sort(key=lambda row: (priority.index(row.tech) if row.tech in priority else len(priority)))
    return [(row.tech, row.building) for row in all_rules
            if row.tech not in ages and rank[ctx.economy.age] >= rank[row.age]
            and (row.prerequisite is None or ctx.economy.has_researched(row.prerequisite))
            and len({b.kind for b in ctx.buildings if b.complete and b.kind in row.building_kinds}) >= row.building_count]
