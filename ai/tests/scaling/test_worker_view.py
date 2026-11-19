"""An idle settlement can get local terrain without growing the bot's view."""
from aoeworld import Age, ResourceKind

from aoe_ai import Brain
from aoe_ai.blackboard import Blackboard
from aoe_ai.config import Config
from aoe_ai.context import Context
from aoe_ai.play.constructor import Constructor
from aoe_ai.play.economy import EconomyManager
from fakes import FakeClient, FakeWorld, base, decision, economy, node, unit
from test_logic import play


def test_a_large_idle_settlement_gets_a_short_view_then_follow_resumes():
    units, buildings = base(0)
    idle = [unit(500 + i, tile=(181 + i % 5, 71 + i // 5)) for i in range(40)]
    world = FakeWorld(units + idle, buildings, size=(500, 500))
    world.land_known = lambda x, y: x < 128
    config = Config(late_economy_min_villagers=32)
    _, early = play(world, config)
    assert early.named("focus_view") == []

    world.economy.age = Age.CASTLE
    brain, first = play(world, config)
    assert first.named("focus_view") == [((160, 96),)]

    world.tick += world.tick_hz
    _, midway = play(world, brain=brain)
    assert midway.named("focus_view") == []

    world.tick += world.tick_hz
    _, restored = play(world, brain=brain)
    assert restored.named("focus_view") == [()]

    world.tick += 30 * world.tick_hz
    world.land_known = lambda *_: True
    _, known = play(world, brain=brain)
    assert known.named("focus_view") == []


def test_one_isolated_worker_cannot_take_the_large_idle_roster_route_budget():
    units, buildings = base(0)
    blocked = unit(300, tile=(45, 50))
    good = unit(301, tile=(52, 49))
    rest = [unit(350 + i, tile=(48, 52)) for i in range(20)]
    trees = [node(700 + i, ResourceKind.WOOD, (55, 48 + i)) for i in range(10)]
    world = FakeWorld(units + [blocked, good] + rest, buildings, nodes=trees,
                      eco=economy(food=0, wood=0, age=Age.CASTLE))
    checked = []

    def find(x, y, tx, ty):
        checked.append(((x, y), (tx, ty)))
        return [(x, y), (tx, ty)] if (x, y) == good.tile else None

    world.land_route = find
    world.land_frontier = lambda *_: None
    config = Config(land_queries_per_decision=4, late_economy_min_villagers=16)
    ctx = Context(FakeClient(), decision(world), Blackboard(), config)
    EconomyManager(Constructor()).assign_idle(ctx)
    assert any(orders[0] == good.id for orders in ctx.client.named("gather"))
    assert len(checked) <= 4
