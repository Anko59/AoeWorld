"""Five AI drafts: a strategy is a :class:`Config` (every tunable lives there).

``python -m aoe_ai tournament`` plays them against each other;
``python -m aoe_ai host --draft NAME`` or ``AOE_AI_DRAFT=NAME`` plays one in a
real match. The scoreboard of the latest runs is ``SCOREBOARD.md`` here.
"""

from __future__ import annotations

from dataclasses import replace
from typing import Callable

from ..config import Config


#: Feudal shares of the drafts: food first, little gold until the Castle is bought.
FEUDAL_SHARES = {"FOOD": 0.45, "WOOD": 0.42, "GOLD": 0.13, "STONE": 0.0}


def quick(config: Config) -> Config:
    """The Castle-first timing every draft shares: Feudal click at 19 villagers,
    Castle price saved from 30, soldiers wait for it."""
    return replace(config, house_pending=3, castle_villagers=min(config.castle_villagers, 28), feudal_villagers=19, castle_first=True, castle_army_min=0,
                   feudal_shares=dict(FEUDAL_SHARES))


def baseline() -> Config:
    """The long-standing default: a standard boom into a Feudal army."""
    return Config()


def boom() -> Config:
    """Boom into Imperial: villagers without pause, a second Town Center, one
    Steward and a Castle with a Vassal to hold the base; the army waits."""
    return replace(
        Config(), name="boom", base_villagers=250, feudal_villagers=24, castle_villagers=34,
        castle_army_min=8, imperial_villagers=60, imperial_army_min=12, army_min_villagers=30,
        attack_army_size=40, town_centers=6, expand_villagers=30, castles=1, vassals=1, stewards=2,
        steward_min_villagers=30, town_center_queue=3,
    )


def fast_castle() -> Config:
    """Fast Castle with Vassals: early ages, two Castles each with a Vassal that
    raises towers, a barracks and walls while the economy keeps growing."""
    return replace(
        Config(), name="fast_castle", base_villagers=150, feudal_villagers=20, castle_villagers=30,
        castle_army_min=4, imperial_villagers=40, imperial_army_min=10, castles=2, vassals=2, town_centers=3, expand_villagers=30,
        stewards=0, attack_army_size=24, castle_knight_weight=2.0,
    )


def generals() -> Config:
    """Military pressure with Generals: a Castle early, two Generals who buy a
    cavalry-heavy army themselves and march it against the nearest enemy."""
    return replace(
        Config(), name="generals", base_villagers=150, town_centers=3, expand_villagers=30, castle_villagers=30, castle_army_min=8,
        imperial_villagers=40, imperial_army_min=24, castles=1, vassals=0, generals=2,
        general_attack_army=30, general_retreat_army=10, general_mix=(30, 25, 35, 10, 0),
        castle_knight_weight=3.0, attack_army_size=30, max_barracks=3,
    )


def steward_scaler() -> Config:
    """Steward-heavy economy scaling: three Town Centers each with a Steward
    that manages its villagers; the economy is left to them."""
    return replace(
        Config(), name="steward_scaler", base_villagers=400, feudal_villagers=22, castle_villagers=34,
        castle_army_min=8, imperial_villagers=80, imperial_army_min=14, army_min_villagers=30,
        town_centers=10, expand_villagers=30, expand_distance=26, stewards=4, steward_min_villagers=28,
        defer_to_steward=True, castles=1, vassals=1, town_center_queue=3, attack_army_size=40,
    )


def balanced() -> Config:
    """Balanced and adaptive: every management unit in moderation. Starts as a
    boom and, once an enemy has been seen near home, answers with Generals."""
    return replace(
        Config(), name="balanced", base_villagers=220, feudal_villagers=22, castle_villagers=32,
        castle_army_min=10, imperial_villagers=55, imperial_army_min=18, town_centers=5,
        expand_villagers=30, stewards=1, steward_min_villagers=32, castles=2, vassals=1,
        generals=1, general_attack_army=36, attack_army_size=36,
    )


DRAFTS: dict[str, Callable[[], Config]] = {
    "baseline": baseline, "boom": lambda: quick(boom()), "fast_castle": lambda: quick(fast_castle()),
    "generals": lambda: quick(generals()), "steward_scaler": lambda: quick(steward_scaler()),
    "balanced": lambda: quick(balanced()),
}


def draft(name: str) -> Config:
    try:
        return DRAFTS[name]()
    except KeyError:
        raise SystemExit(f"unknown draft {name!r}; choose from {', '.join(DRAFTS)}") from None
