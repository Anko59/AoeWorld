"""The build order: which stage the game is in and what it asks for next.

Dark Age: boom (villagers without pause, houses, a Lumber Camp and a Mill),
Loom, then at about twenty villagers save 500 food and click Feudal Age.
Feudal Age: Barracks, Archery Range, Blacksmith, the economy upgrades and a
mixed army. The managers only *execute* this plan.
"""

from __future__ import annotations

from typing import Callable

from aoeworld import BuildingKind, Tech

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


def requirement_kinds(ctx: Context) -> int:
    """Distinct finished kinds that count toward Feudal Age."""
    return len({b.kind for b in ctx.buildings if b.complete and b.kind in FEUDAL_REQUIREMENT})


def stage(ctx: Context) -> str:
    if ctx.feudal:
        return FEUDAL
    if len(ctx.villagers) >= ctx.config.feudal_villagers and requirement_kinds(ctx) >= 2:
        return ADVANCE
    return DARK


def wants_loom(ctx: Context) -> bool:
    return len(ctx.villagers) >= ctx.config.loom_villagers


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
    wants = [
        (BuildingKind.BARRACKS, 1),
        (BuildingKind.ARCHERY_RANGE, 1),
        (BuildingKind.BLACKSMITH, 1),
    ]
    villagers = len(ctx.villagers)
    rich = ctx.economy.wood > 450 and ctx.economy.food > 300 and villagers >= 24
    if rich:
        wants += [
            (BuildingKind.BARRACKS, ctx.config.max_barracks),
            (BuildingKind.ARCHERY_RANGE, ctx.config.max_ranges),
        ]
    return wants


#: A building that must be finished before another can be placed.
PREREQUISITE = {BuildingKind.ARCHERY_RANGE: BuildingKind.BARRACKS}


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


def wanted_upgrades(ctx: Context) -> list[tuple[Tech, BuildingKind]]:
    if not ctx.feudal:
        return []
    return [(tech, kind) for tech, kind, when in UPGRADES if when(ctx)]
