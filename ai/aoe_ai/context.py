"""One decision's view: the world, the budget and order helpers."""

from __future__ import annotations

from typing import Any, Optional

import aoeworld
from aoeworld import BuildingKind, ResourceKind, UnitKind

from .config import Config
from .geometry import Tile, clamp_tile, dist
from .blackboard import Blackboard, Tracked

MILITARY = (
    UnitKind.MILITIA,
    UnitKind.SPEARMAN,
    UnitKind.ARCHER,
    UnitKind.SKIRMISHER,
    UnitKind.MAN_AT_ARMS,
    UnitKind.LONG_SWORDSMAN, UnitKind.PIKEMAN, UnitKind.CROSSBOWMAN,
    UnitKind.ELITE_SKIRMISHER, UnitKind.KNIGHT, UnitKind.LIGHT_CAVALRY,
    UnitKind.MANGONEL, UnitKind.BATTERING_RAM,
)

#: Buildings that accept each resource as a drop-off.
#: Orders whose first argument names the unit(s) given work.
UNIT_ORDERS = frozenset({"gather", "hunt", "farm", "build", "build_bridge", "construct", "move", "formation_move", "attack", "stop"})

DROPOFFS = {
    ResourceKind.FOOD: (BuildingKind.TOWN_CENTER, BuildingKind.MILL),
    ResourceKind.WOOD: (BuildingKind.TOWN_CENTER, BuildingKind.LUMBER_CAMP),
    ResourceKind.GOLD: (BuildingKind.TOWN_CENTER, BuildingKind.MINING_CAMP),
    ResourceKind.STONE: (BuildingKind.TOWN_CENTER, BuildingKind.MINING_CAMP),
}


class Context:
    """Everything a manager needs for one decision, computed once."""

    def __init__(self, client: Any, decision: Any, board: Blackboard, config: Config) -> None:
        self.client = client
        self.world = decision.world
        self.events = decision.events
        self.board = board
        self.config = config
        self.tick: int = decision.world.tick
        self.economy = decision.world.economy
        self.units = decision.world.my_units()
        self.villagers = [u for u in self.units if u.kind == UnitKind.VILLAGER]
        self.scouts = [u for u in self.units if u.kind == UnitKind.SCOUT]
        self.army = [u for u in self.units if u.kind in MILITARY]
        self.buildings = decision.world.my_buildings()
        self.enemies = decision.world.visible_enemies()
        self.center = decision.world.town_center()
        if self.center is not None and board.home is None:
            board.home = self.center.center
        self.home: Optional[Tile] = self.center.center if self.center else board.home
        self.queued = sum(b.queue_length for b in self.buildings)
        self.spent = [0, 0, 0, 0]
        #: Set by a manager that needs the Town Center's queue to drain.
        self.hold_town_center = False
        self.saving_castle = False
        #: Production kinds that must not start new training this decision.
        self.draining: set = set()
        #: Units that already got an order this decision.
        self.busy: set[int] = set()
        self._resources: dict[Any, list] = {}
        self._spots: dict[Any, list] = {}
        self._land_routes: dict[tuple[Tile, Tile], Any] = {}
        self._land_queries = 0
        army_ids = {unit.id for unit in self.army if unit.alive}
        board.marched = {i: tick for i, tick in board.marched.items() if i in army_ids}
        board.land_failures = {key: until for key, until in board.land_failures.items() if until > self.tick}

    def land_route(self, origin: Tile, target: Tile) -> Optional[list[Tile]]:
        key = (origin, target)
        if key not in self._land_routes:
            if key in self.board.land_failures:
                return None
            if self._land_queries >= self.config.land_queries_per_decision:
                return None
            self._land_queries += 1
            self._land_routes[key] = self.world.land_route(*origin, *target)
            if self._land_routes[key] is None:
                if len(self.board.land_failures) >= self.config.land_cache_entries:
                    self.board.land_failures.clear()
                self.board.land_failures[key] = self.tick + self.config.land_retry_seconds * self.world.tick_hz
        return self._land_routes[key]

    def land_hop(self, origin: Tile, target: Tile) -> Tile:
        """A short proven land hop, shared by marching and rallying soldiers."""
        path = self.land_route(origin, target)
        if path is None:
            return origin
        length, stop = 0.0, origin
        for before, after in zip(path, path[1:]):
            length += dist(before, after)
            if length > self.config.march_hop_tiles:
                break
            stop = after
        stop = self.unrefused(stop)
        return stop if self.land_route(origin, stop) is not None else origin

    def land_frontier(self, origin: Tile, target: Tile) -> Optional[Tile]:
        if self._land_queries >= self.config.land_queries_per_decision:
            return None
        self._land_queries += 1
        return self.world.land_frontier(*origin, *target)

    # ----- queries ---------------------------------------------------------
    @property
    def feudal(self) -> bool:
        return self.economy is not None and self.economy.age in (aoeworld.Age.FEUDAL, aoeworld.Age.CASTLE)

    @property
    def castle(self) -> bool:
        return self.economy is not None and self.economy.age == aoeworld.Age.CASTLE

    def count(self, kind: BuildingKind, complete: bool = False) -> int:
        return sum(1 for b in self.buildings if b.kind == kind and (b.complete or not complete))

    def resources(self, kind: ResourceKind) -> list:
        if kind not in self._resources:
            bad = self.board.bad_resources
            if len(bad) > 8192:
                bad.clear()  # bounded: refused nodes are learned again if still bad
            self._resources[kind] = [
                n for n in self.world.resources(kind) if n.id not in bad
            ]
        return self._resources[kind]

    def dropoff_distance(self, node: Any) -> float:
        """Tiles from a node to the nearest finished building that takes it."""
        spots = self._spots.get(node.kind)
        if spots is None:
            kinds = DROPOFFS[node.kind]
            spots = self._spots[node.kind] = [
                b.center for b in self.buildings if b.complete and b.kind in kinds
            ]
        if not spots:
            return 1e9
        return min(dist(node.tile, s) for s in spots)

    def unrefused(self, tile: Tile) -> Tile:
        """`tile`, or the nearest tile around it the server has not refused as a
        destination (a tree, water or a building: asking again changes nothing)."""
        bad = self.board.bad_moves
        size = self.world.map_size
        if tile not in bad:
            return tile
        for radius in (2, 4, 6, 9, 13):
            for dx, dy in ((1, 0), (0, 1), (-1, 0), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)):
                candidate = clamp_tile((tile[0] + dx * radius, tile[1] + dy * radius), size)
                if candidate not in bad:
                    return candidate
        return tile

    # ----- budget ----------------------------------------------------------
    def can_afford(self, cost: tuple[int, int, int, int]) -> bool:
        if self.economy is None:
            return False
        have = (self.economy.food, self.economy.wood, self.economy.gold, self.economy.stone)
        return all(have[i] - self.spent[i] >= cost[i] for i in range(4))

    def spend(self, cost: tuple[int, int, int, int]) -> None:
        for i in range(4):
            self.spent[i] += cost[i]

    def try_spend(self, cost: tuple[int, int, int, int]) -> bool:
        if self.can_afford(cost):
            self.spend(cost)
            return True
        return False

    def free_population(self) -> int:
        if self.economy is None:
            return 0
        return self.economy.population_cap - self.economy.population - self.queued

    # ----- orders ----------------------------------------------------------
    def order(self, name: str, *args: Any) -> int:
        """Send an order and remember it until its verdict arrives."""
        if name in UNIT_ORDERS and args:
            first = args[0]
            self.busy.update(first if isinstance(first, (list, tuple)) else (first,))
        sequence = getattr(self.client, name)(*args)
        tracked = self.board.tracked
        tracked[sequence] = Tracked(name, args, self.tick)
        if len(tracked) > 4096:
            for stale in sorted(tracked)[:1024]:
                del tracked[stale]
        self.board.bump("orders")
        self.board.bump("order:" + name)
        return sequence
