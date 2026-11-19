"""Cached planning tables from the public SDK, shared across AI slots."""

from collections import Counter
from functools import cache, lru_cache

from aoeworld import (Age, BuildingKind, UnitKind, technologies, tech_cost,
                      trainable, training_options, unit_building_bonus,
                      unit_cost, unit_counter_bonus, unit_profile)

profile = cache(unit_profile)
price = cache(unit_cost)
research_price = cache(tech_cost)
technology_rows = cache(technologies)
counter_bonus = cache(unit_counter_bonus)
building_bonus = cache(unit_building_bonus)
AGE_RANK = {age: rank for rank, age in enumerate((Age.DARK, Age.FEUDAL, Age.CASTLE, Age.IMPERIAL))}


@cache
def land_soldier(kind):
    # The starting Scout explores. Generals have their own production and orders.
    row = profile(kind)
    return row.armed and not row.naval and kind not in (UnitKind.VILLAGER, UnitKind.SCOUT, UnitKind.GENERAL)


@cache
def warship(kind):
    row = profile(kind)
    return row.naval and row.armed


@cache
def producers():
    return {kind: tuple(trainable(kind)) for name in dir(BuildingKind) if name.isupper()
            for kind in (getattr(BuildingKind, name),)}


@cache
def land_producers():
    return frozenset(kind for kind, units in producers().items() if any(map(land_soldier, units)))


@lru_cache(maxsize=4096)
def options(building, age, researched):
    return tuple(training_options(building, age, list(researched)))


def visible_composition(units, select):
    return Counter(unit.kind for unit in units if unit.alive and select(unit.kind))


def best_recruit(ctx, choices, have, enemies, target=None, preferences=None):
    """Fill an underrepresented line, with a bonus for known matchups."""
    enemy_size = sum(enemies.values())
    best, best_score = None, 0
    for kind in choices:
        if str(kind) in ctx.board.blocked_units or not ctx.can_afford(price(kind)):
            continue
        row = profile(kind)
        weight = preferences.get(row.line, ctx.config.army_line_weight) if preferences else ctx.config.army_line_weight
        if enemy_size:
            counter = sum(n * min(counter_bonus(kind, foe), ctx.config.army_counter_bonus_cap)
                          for foe, n in enemies.items())
            weight *= 1 + ctx.config.army_counter_weight * counter / (enemy_size * ctx.config.army_counter_bonus_cap)
        if target is not None and row.siege:
            bonus = min(building_bonus(kind, target), ctx.config.army_siege_bonus_cap)
            weight *= 1 + ctx.config.army_siege_weight * bonus / ctx.config.army_siege_bonus_cap
        score = weight / (have[row.line] + 1)
        if score > best_score:
            best, best_score = kind, score
    return best
