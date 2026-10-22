"""Every start site of a match carries the standard Age of Empires II start.

The official Arabia guide lists, per player, 8 herdables, 3 forests, 6 fruit
bushes, 2 aggressive and 4 wild hunt, 7 tiles of gold and 5 of stone
(https://ageofempires.com/learn-to-play/defending-your-empires-aoe2). The
resources lie in fog at the start, so each scout walks rings round its base
first; only what a human could see is counted. Starting-kit nodes are not in
any map chunk: the SDK must build them from the resource feed.
"""

import math

import aoeworld
from aoeworld import AnimalKind, ResourceKind

from aoe_ai.geometry import ring_points
from aoe_ai.host import Host

KIT_RADIUS = 18


class Surveyor:
    """A brain that only scouts around its Town Center and keeps what it saw."""

    config = type("C", (), {"decision_hz": 2.0})()
    board = type("M", (), {"stats": {}})()

    def __init__(self):
        self.nodes = {}
        self.animals = {}
        self.home = None
        self.villagers = 0
        self.scouts = 0
        self.points = []

    def decide(self, client, decision):
        world = decision.world
        center = world.town_center()
        if center is None:
            return
        if self.home is None:
            self.home = center.center
            self.villagers = len(world.my_units(aoeworld.UnitKind.VILLAGER))
            self.scouts = len(world.my_units(aoeworld.UnitKind.SCOUT))
            for radius in (8, 14):
                self.points += ring_points(self.home, radius, spacing=7)
        for scout in world.my_units(aoeworld.UnitKind.SCOUT):
            if scout.idle and self.points:
                x, y = self.points.pop(0)
                client.move(scout.id, x, y)
        for kind in (ResourceKind.FOOD, ResourceKind.WOOD, ResourceKind.GOLD, ResourceKind.STONE):
            for node in world.resources(kind):
                self.nodes[node.id] = node
        for animal in world.animals():
            self.animals[animal.id] = animal

    def near(self, items):
        return [i for i in items if math.dist(i.tile, self.home) <= KIT_RADIUS]


def test_every_slot_gets_the_full_standard_start(server, secret):
    info = aoeworld.start_match(server, secret, ["ai"] * 2, tiles_per_side=500, seed=3)
    host = Host(
        server,
        [slot.token for slot in info.slots],
        brain_factory=lambda slot: Surveyor(),
    )
    host.run(until_tick=20 * 100, max_seconds=200)
    # The exact counts are pinned on the server (gameplay_match tests); here
    # the bots must *see* every part of the kit through the SDK.
    wanted = (
        ResourceKind.FOOD,
        ResourceKind.GOLD,
        ResourceKind.STONE,
        ResourceKind.WOOD,
        AnimalKind.SHEEP,
        AnimalKind.DEER,
        AnimalKind.BOAR,
    )
    short = []
    for slot, brain in enumerate(host.brains):
        assert (brain.villagers, brain.scouts) == (3, 1), f"slot {slot}"
        have = {item.kind for item in brain.near(brain.nodes.values()) + brain.near(brain.animals.values())}
        short += [f"slot {slot} at {brain.home}: no {kind}" for kind in wanted if kind not in have]
    assert not short, short
