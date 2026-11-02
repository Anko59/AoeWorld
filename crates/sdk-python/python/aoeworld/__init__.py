"""Write AoeWorld AI players in Python.

An AI is a separate process that joins one slot of a match with that slot's
token, sees only what its fog of war allows and sends ordinary orders::

    import aoeworld

    client = aoeworld.connect("http://127.0.0.1:8080", slot_token)

    def on_tick(client, decision):
        world = decision.world
        for villager in world.idle_units(aoeworld.UnitKind.VILLAGER):
            node = world.nearest_resource(aoeworld.ResourceKind.WOOD, *villager.tile)
            if node:
                client.gather(villager.id, node.id)

    client.run(on_tick, until_tick=20 * 600)
"""

from ._native import (
    Age,
    AoeError,
    Animal,
    AnimalKind,
    AnimalTask,
    Building,
    BuildingKind,
    Client,
    CommandResult,
    ConnectionClosed,
    Decision,
    Economy,
    Event,
    Fleet,
    Formation,
    CombatStance,
    MatchInfo,
    Refused,
    ResourceKind,
    ResourceNode,
    RunSummary,
    SlotAssignment,
    Tech,
    TradeResource,
    Unit,
    UnitKind,
    GeneralState,
    UnitTask,
    Visibility,
    World,
    build_seconds,
    building_cost,
    building_buildable,
    building_required_age,
    building_requires,
    building_size,
    connect,
    start_match,
    tech_cost,
    train_seconds,
    trainable,
    unit_cost,
    villager_capacity,
)
from .aio import AsyncFleet

__version__ = "0.1.0"

__all__ = [
    "Age",
    "Animal",
    "AnimalKind",
    "AnimalTask",
    "AoeError",
    "AsyncFleet",
    "Building",
    "BuildingKind",
    "Client",
    "CommandResult",
    "ConnectionClosed",
    "Decision",
    "Economy",
    "Event",
    "Fleet",
    "Formation",
    "CombatStance",
    "MatchInfo",
    "Refused",
    "ResourceKind",
    "ResourceNode",
    "RunSummary",
    "SlotAssignment",
    "Tech",
    "TradeResource",
    "Unit",
    "UnitKind",
    "GeneralState",
    "UnitTask",
    "Visibility",
    "World",
    "build_seconds",
    "building_cost",
    "building_buildable",
    "building_required_age",
    "building_requires",
    "building_size",
    "connect",
    "start_match",
    "tech_cost",
    "train_seconds",
    "trainable",
    "unit_cost",
    "villager_capacity",
]
