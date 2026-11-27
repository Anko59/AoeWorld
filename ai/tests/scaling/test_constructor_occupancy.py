"""The constructor leaves occupied foundations alone until the footprint clears."""

from aoeworld import BuildingKind, UnitKind

from aoe_ai.blackboard import Blackboard
from aoe_ai.config import Config
from aoe_ai.context import Context
from aoe_ai.play.constructor import Constructor
from fakes import FakeClient, FakeWorld, base, building, decision, unit


def test_a_stalled_foundation_is_restaffed_only_after_visible_blockers_leave():
    units, buildings = base(3)
    buildings.append(building(901, BuildingKind.HOUSE, tile=(57, 50), complete=False))
    blocker = unit(902, UnitKind.MILITIA, (58, 50), mine=False)
    world = FakeWorld(units, buildings, enemies=[blocker])
    world.tick = 1000
    board = Blackboard()
    board.progress[901] = (0, 0)

    blocked = Context(FakeClient(), decision(world), board, Config())
    Constructor().finish_foundations(blocked)
    assert not blocked.client.named("construct")

    blocker.tile = (60, 50)
    clear = Context(FakeClient(), decision(world), board, Config())
    Constructor().finish_foundations(clear)
    assert [args[1] for args in clear.client.named("construct")] == [901]
