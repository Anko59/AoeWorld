from aoeworld import Age, BuildingKind, UnitKind
from aoe_ai.blackboard import Blackboard
from aoe_ai.config import Config
from aoe_ai.context import Context
from aoe_ai.play.bridges import BridgeManager
from fakes import FakeClient, FakeWorld, base, building, decision, economy, unit


def setup(enabled=True, stone=200):
    units, buildings = base(2)
    units.append(unit(200, UnitKind.MILITIA, (60, 50)))
    target = building(900, BuildingKind.TOWN_CENTER, (90, 50), mine=False)
    world = FakeWorld(units, buildings, economy(wood=500, stone=stone, age=Age.FEUDAL),
                      enemy_buildings=[target])
    world.land_route = lambda x, y, tx, ty: None if tx > 65 else [(x, y), (tx, ty)]
    world.land_frontier = lambda *args: (65, 50)
    world.find_bridge_site = lambda *args: ((65, 50), (70, 50))
    client = FakeClient()
    return Context(client, decision(world), Blackboard(), Config(build_bridges=enabled)), client


def test_optional_bridge_orders_use_a_reachable_villager_and_span_cost():
    ctx, client = setup()
    BridgeManager().step(ctx)
    assert len(client.named("build_bridge")) == 1
    builders, *span = client.named("build_bridge")[0]
    assert len(builders) == 1
    assert span == [65, 50, 70, 50]
    assert ctx.spent == [0, 100, 0, 20]
    assert builders[0] in ctx.busy
    BridgeManager().step(ctx)
    assert len(client.named("build_bridge")) == 1, "cooldown bounds repeated decisions"


def test_bridge_heuristic_is_disabled_by_default_and_respects_resources_and_known_sites():
    assert not Config().build_bridges
    for enabled, stone in [(False, 200), (True, 0)]:
        ctx, client = setup(enabled, stone)
        BridgeManager().step(ctx)
        assert not client.named("build_bridge")
    ctx, client = setup()
    ctx.world.find_bridge_site = lambda *args: None
    BridgeManager().step(ctx)
    assert not client.named("build_bridge")


def test_bridge_refusals_release_the_plan_and_builder():
    from aoe_ai.brain import Brain
    from aoeworld import CommandResult
    for result in (CommandResult.REJECTED_INVALID_PLACEMENT, CommandResult.REJECTED_WORLD_LIMIT):
        ctx, _ = setup()
        BridgeManager().step(ctx)
        tracked = next(iter(ctx.board.tracked.values()))
        Brain().on_rejected(ctx, tracked, result)
        assert not ctx.board.plans
        assert not ctx.board.builders
