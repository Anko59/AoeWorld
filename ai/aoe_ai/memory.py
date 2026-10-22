"""What a bot remembers between decisions."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Optional

Tile = tuple[int, int]


@dataclass
class Tracked:
    """An order whose verdict has not arrived yet."""

    name: str
    args: tuple
    tick: int


@dataclass
class Plan:
    """A building the bot intends to have (placed but maybe not yet visible)."""

    kind: Any  # aoeworld.BuildingKind
    tile: Tile
    tick: int


@dataclass
class BotMemory:
    tracked: dict[int, Tracked] = field(default_factory=dict)
    #: Resource nodes a gather order was refused for (fish, unreachable...).
    bad_resources: set[int] = field(default_factory=set)
    #: (kind name, x, y) sites the server refused.
    bad_sites: set[tuple[str, int, int]] = field(default_factory=set)
    #: Unit kinds the server refused to train (cleared when the age changes).
    blocked_units: set[str] = field(default_factory=set)
    plans: list[Plan] = field(default_factory=list)
    #: farm id -> farmer id
    farmers: dict[int, int] = field(default_factory=dict)
    #: villager id -> (what, tile) job description while a build is under way
    builders: dict[int, tuple[str, Tile]] = field(default_factory=dict)
    scout_waypoints: list[Tile] = field(default_factory=list)
    scout_index: int = 0
    scout_radius: int = 0
    home: Optional[Tile] = None
    #: Ticks at which a thing was last tried, to avoid hammering refusals.
    cooldowns: dict[str, int] = field(default_factory=dict)
    threat_until: int = -1
    threat_tile: Optional[Tile] = None
    attack_launch_size: int = 0
    attack_target: Optional[int] = None
    attacking: bool = False
    #: Counters for reports and tests.
    stats: dict[str, int] = field(default_factory=dict)

    def bump(self, name: str, amount: int = 1) -> None:
        self.stats[name] = self.stats.get(name, 0) + amount

    def cooling(self, name: str, tick: int) -> bool:
        return self.cooldowns.get(name, -1) > tick

    def cool(self, name: str, tick: int, ticks: int) -> None:
        self.cooldowns[name] = tick + ticks
