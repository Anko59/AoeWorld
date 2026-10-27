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
class Blackboard:
    """Everything the managers share between decisions: orders in flight,
    refusals learned, plans, scouting progress and the attack state."""

    tracked: dict[int, Tracked] = field(default_factory=dict)
    #: Resource nodes a gather order was refused for (fish, unreachable...).
    bad_resources: set[int] = field(default_factory=set)
    #: (kind name, x, y) sites the server refused.
    bad_sites: set[tuple[str, int, int]] = field(default_factory=set)
    #: Building targets abandoned after a stalled land approach.
    cleared: set[int] = field(default_factory=set)
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
    scout_stage: int = 0
    #: The army's march: goal, nearest the army got to it, and since when.
    march_goal: Optional[Tile] = None
    march_best: float = 0.0
    march_since: int = 0
    #: soldier id -> tick of its last march order
    marched: dict[int, int] = field(default_factory=dict)
    #: soldier id -> (last progress tile, tick); MOVING alone is not progress.
    march_progress: dict[int, tuple[Tile, int]] = field(default_factory=dict)
    #: villager id -> (resource node, tick) of its last gather order
    gather_sent: dict[int, tuple[int, int]] = field(default_factory=dict)
    #: scout id -> the waypoint it was last sent to
    scout_sent: dict[int, int] = field(default_factory=dict)
    scout_targets: dict[int, Tile] = field(default_factory=dict)
    home: Optional[Tile] = None
    #: Ticks at which a thing was last tried, to avoid hammering refusals.
    cooldowns: dict[str, int] = field(default_factory=dict)
    threat_until: int = -1
    threat_tile: Optional[Tile] = None
    attack_launch_size: int = 0
    attack_target: Optional[int] = None
    attacking: bool = False
    #: Which far point the army marches to when no enemy building is known.
    search_index: int = 0
    #: Destinations a move order was refused for.
    bad_moves: set[tuple[int, int]] = field(default_factory=set)
    #: Failed public land queries are retried after a short bounded cooldown.
    land_failures: dict[tuple[tuple[int, int], tuple[int, int]], int] = field(default_factory=dict)
    #: villager id -> tick of its last job, (villager, animal) -> hunt orders given.
    assigned: dict[int, int] = field(default_factory=dict)
    hunt_tries: dict[tuple[int, int], int] = field(default_factory=dict)
    #: villager id -> animal it was sent after; animal id -> ((hp, meat), tick it last changed).
    hunters: dict[int, int] = field(default_factory=dict)
    hunt_watch: dict[int, tuple[tuple[int, int], int]] = field(default_factory=dict)
    #: Enemy units and buildings seen below full health (our hits landing).
    wounded: set[int] = field(default_factory=set)
    #: Own foundation id -> (progress, tick it last advanced).
    progress: dict[int, tuple[int, int]] = field(default_factory=dict)
    #: Enemy unit or building id -> hit points when last seen.
    enemy_hp: dict[int, int] = field(default_factory=dict)
    #: Counters for reports and tests.
    stats: dict[str, int] = field(default_factory=dict)

    def bump(self, name: str, amount: int = 1) -> None:
        self.stats[name] = self.stats.get(name, 0) + amount

    def cooling(self, name: str, tick: int) -> bool:
        return self.cooldowns.get(name, -1) > tick

    def cool(self, name: str, tick: int, ticks: int) -> None:
        # Expired cooldowns are dropped, so the table is bounded by what is
        # cooling now, not by the length of the game.
        if len(self.cooldowns) >= 2048:
            self.cooldowns = {k: v for k, v in self.cooldowns.items() if v > tick}
        self.cooldowns[name] = tick + ticks
