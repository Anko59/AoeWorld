"""Stand-ins for the native world, so decision logic is testable in isolation.

Enums are the real ``aoeworld`` enums; entities are plain namespaces with the
same attributes the native objects have.
"""

from __future__ import annotations

from types import SimpleNamespace as NS
from typing import Any

import aoeworld
from aoeworld import AnimalKind, AnimalTask, Age, BuildingKind, ResourceKind, UnitKind, UnitTask


def unit(id, kind=UnitKind.VILLAGER, tile=(50, 50), task=UnitTask.IDLE, gathering=None, mine=True):
    x, y = tile
    return NS(
        id=id, kind=kind, x=x, y=y, tile=tile, mine=mine, owner=0 if mine else 1,
        task=task, gathering=gathering, idle=task == UnitTask.IDLE, alive=True,
        military=kind in (UnitKind.SPEARMAN, UnitKind.ARCHER, UnitKind.SKIRMISHER, UnitKind.MILITIA),
        hp=25, max_hp=25, attack_target=None, carrying=None, moving=False,
    )


def building(id, kind, tile=(50, 50), complete=True, queue=0, researching=None, mine=True, visible=True, hp=1000):
    w, h = aoeworld.building_size(kind)
    return NS(
        id=id, kind=kind, x=tile[0], y=tile[1], width=w, height=h,
        center=(tile[0] + w // 2, tile[1] + h // 2), complete=complete, queue_length=queue,
        researching=researching, idle=complete and queue == 0 and researching is None,
        mine=mine, visible=visible, owner=0 if mine else 1, hp=hp, max_hp=1000,
        progress=0 if not complete else 1000, farm_reseeds=0,
    )


def node(id, kind, tile, remaining=100):
    return NS(id=id, kind=kind, x=tile[0], y=tile[1], tile=tile, remaining=remaining)


def animal(id, tile, kind=AnimalKind.SHEEP, food=100):
    return NS(id=id, kind=kind, x=tile[0], y=tile[1], tile=tile, food_remaining=food, owner=None,
              task=AnimalTask.IDLE, hp=7)


def economy(food=200, wood=200, gold=100, stone=200, population=4, cap=10, age=Age.DARK, researched=()):
    return NS(
        food=food, wood=wood, gold=gold, stone=stone, population=population, population_cap=cap,
        age=age, researched=list(researched), free_population=cap - population,
        has_researched=lambda tech: tech in researched,
    )


class FakeWorld:
    def __init__(self, units=(), buildings=(), eco=None, nodes=(), animals=(), enemies=(),
                 enemy_buildings=(), tick=100, size=(300, 300)):
        self.tick = tick
        self.tick_hz = 20
        self.me = 0
        self.map_size = size
        self.economy = eco or economy()
        self._units = list(units)
        self._buildings = list(buildings)
        self._nodes = list(nodes)
        self._animals = list(animals)
        self._enemies = list(enemies)
        self._enemy_buildings = list(enemy_buildings)
        self.sites: list[tuple] = []

    def land_route(self, x, y, tx, ty):
        path = [(x, y)]
        while (x, y) != (tx, ty):
            x += (tx > x) - (tx < x)
            y += (ty > y) - (ty < y)
            path.append((x, y))
        return path

    def land_frontier(self, x, y, tx, ty):
        path = self.land_route(x, y, tx, ty)
        return path[-1] if path else None

    def my_units(self, kind=None):
        return [u for u in self._units if kind is None or u.kind == kind]

    def my_buildings(self, kind=None, complete_only=False):
        return [b for b in self._buildings if (kind is None or b.kind == kind) and (b.complete or not complete_only)]

    def town_center(self):
        return next((b for b in self._buildings if b.kind == BuildingKind.TOWN_CENTER and b.complete), None)

    def visible_enemies(self):
        return list(self._enemies)

    def known_enemy_buildings(self):
        return list(self._enemy_buildings)

    def resources(self, kind=None):
        return [n for n in self._nodes if kind is None or n.kind == kind]

    def animals(self):
        return list(self._animals)

    def find_build_site(self, kind, x, y, max_radius=24, gap=1):
        self.sites.append((kind, x, y))
        return (x + 3, y + 3)


class FakeClient:
    """Records orders; returns increasing sequence numbers like the real one."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, tuple]] = []
        self.options: dict[int, dict] = {}

    def __getattr__(self, name: str) -> Any:
        def call(*args, **options):
            self.calls.append((name, args))
            if options:
                self.options[len(self.calls)] = options
            return len(self.calls)
        return call

    def options_of(self, name: str) -> list[dict]:
        """Keyword options of each call to `name`, in call order."""
        return [self.options.get(i + 1, {}) for i, (n, _) in enumerate(self.calls) if n == name]

    def named(self, name: str) -> list[tuple]:
        return [args for n, args in self.calls if n == name]


def decision(world, events=()):
    return NS(world=world, events=list(events), tick=world.tick)


def order_result(sequence, result):
    return NS(kind="order_result", sequence=sequence, result=result)


def base(villagers=12, **kwargs):
    """A Town Center and some villagers around it."""
    units = [unit(i, tile=(48 + i % 4, 52 + i // 4)) for i in range(1, villagers + 1)]
    units.append(unit(100, UnitKind.SCOUT, tile=(50, 50)))
    buildings = [building(500, BuildingKind.TOWN_CENTER, tile=(50, 50))]
    return units, buildings
