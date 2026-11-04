"""The build order: which stage the game is in and what it asks for next.

Dark Age: boom (villagers without pause, a Lumber Camp and a Mill),
Loom, then at about twenty villagers save 500 food and click Feudal Age.
Feudal Age: Barracks, Archery Range, Blacksmith, the economy upgrades and a
mixed army. The managers only *execute* this plan.
"""

from __future__ import annotations

from typing import Callable

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
        wants.insert(2, (BuildingKind.STABLE, 1))
    # The Imperial Age buildings come before the second Barracks and Range:
    # they are the age's prerequisite, the extra production is not.
    if ctx.castle and not ctx.imperial and len(ctx.villagers) >= ctx.config.imperial_villagers - 6:
        wants.extend((kind, 1) for kind in IMPERIAL_BUILDINGS)
    if len(ctx.villagers) >= 22:
        wants.append((BuildingKind.BARRACKS, ctx.config.max_barracks))
    if len(ctx.villagers) >= 26:
        wants.append((BuildingKind.ARCHERY_RANGE, ctx.config.max_ranges))
    return wants


#: A building that must be finished before another can be placed.
PREREQUISITE = {BuildingKind.ARCHERY_RANGE: BuildingKind.BARRACKS, BuildingKind.STABLE: BuildingKind.BARRACKS,
                BuildingKind.SIEGE_WORKSHOP: BuildingKind.BLACKSMITH}


# ----- technologies -------------------------------------------------------
def _farms(ctx: Context) -> bool:
    return ctx.count(BuildingKind.FARM) >= 2


def _miners(ctx: Context) -> bool:
    return len(ctx.villagers) >= 24


def _army(size: int) -> Callable[[Context], bool]:
    return lambda ctx: len(ctx.army) >= size


#: (tech, building that researches it, condition), in priority order.
UPGRADES: tuple[tuple[Tech, BuildingKind, Callable[[Context], bool]], ...] = (
    (Tech.DOUBLE_BIT_AXE, BuildingKind.LUMBER_CAMP, lambda ctx: True),
    (Tech.HORSE_COLLAR, BuildingKind.MILL, _farms),
    (Tech.WHEELBARROW, BuildingKind.TOWN_CENTER, lambda ctx: len(ctx.villagers) >= 24),
    (Tech.GOLD_MINING, BuildingKind.MINING_CAMP, _miners),
    (Tech.FLETCHING, BuildingKind.BLACKSMITH, _army(6)),
    (Tech.FORGING, BuildingKind.BLACKSMITH, _army(8)),
    (Tech.PADDED_ARCHER_ARMOR, BuildingKind.BLACKSMITH, _army(10)),
    (Tech.SCALE_MAIL_ARMOR, BuildingKind.BLACKSMITH, _army(10)),
)


def _has(tech: Tech) -> Callable[[Context], bool]:
    return lambda ctx: ctx.economy is not None and ctx.economy.has_researched(tech)


#: Imperial Age upgrades in priority order: the army's lines first (each step
#: waits for the one before it), then armour and economy.
IMPERIAL_UPGRADES: tuple[tuple[Tech, BuildingKind, Callable[[Context], bool]], ...] = (
    (Tech.ARBALESTER, BuildingKind.ARCHERY_RANGE, _has(Tech.CROSSBOWMAN)),
    (Tech.HALBERDIER, BuildingKind.BARRACKS, _has(Tech.PIKEMAN)),
    (Tech.CAVALIER, BuildingKind.STABLE, lambda ctx: True),
    (Tech.BRACER, BuildingKind.BLACKSMITH, _has(Tech.BODKIN_ARROW)),
    (Tech.BLAST_FURNACE, BuildingKind.BLACKSMITH, _has(Tech.IRON_CASTING)),
    (Tech.PLATE_MAIL_ARMOR, BuildingKind.BLACKSMITH, _has(Tech.CHAIN_MAIL_ARMOR)),
    (Tech.RING_ARCHER_ARMOR, BuildingKind.BLACKSMITH, _has(Tech.LEATHER_ARCHER_ARMOR)),
    (Tech.PLATE_BARDING_ARMOR, BuildingKind.BLACKSMITH, _has(Tech.CHAIN_BARDING_ARMOR)),
    (Tech.PALADIN, BuildingKind.STABLE, _has(Tech.CAVALIER)),
    (Tech.CROP_ROTATION, BuildingKind.MILL, _has(Tech.HEAVY_PLOW)),
    (Tech.TWO_MAN_SAW, BuildingKind.LUMBER_CAMP, _has(Tech.BOW_SAW)),
)


def wanted_upgrades(ctx: Context) -> list[tuple[Tech, BuildingKind]]:
    if not ctx.feudal:
        return []
    upgrades = [(tech, kind) for tech, kind, when in UPGRADES if when(ctx)]
    if ctx.castle:
        upgrades = [(Tech.CROSSBOWMAN, BuildingKind.ARCHERY_RANGE),
                    (Tech.PIKEMAN, BuildingKind.BARRACKS)] + upgrades
    if ctx.imperial:
        upgrades = [(tech, kind) for tech, kind, when in IMPERIAL_UPGRADES if when(ctx)] + upgrades
    return upgrades
