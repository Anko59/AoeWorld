"""River decisions use public terrain knowledge and keep the navy small."""

from aoeworld import Age, BuildingKind, ResourceKind, UnitKind

from aoe_ai import Brain, Config
from aoe_ai.blackboard import Blackboard
from aoe_ai.context import Context
from aoe_ai.play.constructor import Constructor
from fakes import FakeClient, FakeWorld, base, building, decision, economy, node, unit


def river_world(units=None, docks=(), nodes=None, tick=100):
    villagers, buildings = base(4)
    world = FakeWorld(
        villagers if units is None else units, buildings + list(docks),
        eco=economy(food=500, wood=1000, gold=500, population=5, age=Age.FEUDAL),
        nodes=[node(700, ResourceKind.FOOD, (56, 52)),
               node(701, ResourceKind.FOOD, (59, 52))] if nodes is None else nodes,
        tick=tick,
    )
    # A seven-tile river. Our villagers can reach the west bank, but neither
    # water tiles nor the east bank: a fish tile itself has no land route.
    world.passable = lambda x, y: not 56 <= x <= 62
    world.land_route = lambda x, y, tx, ty: (
        [(x, y), (tx, ty)] if x <= 55 and tx <= 55 else None
    )
    return world


def context(world, board=None):
    return Context(FakeClient(), decision(world), board or Blackboard(), Config())


def naval():
    from aoe_ai.play.naval import NavalManager
    return NavalManager(Constructor())


def test_villager_selects_shore_fish_via_the_reachable_bank():
    ctx = context(river_world())
    manager = Brain().economy
    selected = manager.best_node(ctx, ctx.villagers[0], ResourceKind.FOOD)
    assert selected is not None and selected.id == 700
    assert manager.send_to_food(ctx, ctx.villagers[0])
    assert ctx.client.named("gather") == [(ctx.villagers[0].id, 700)]


def test_villagers_leave_deep_and_opposite_bank_fish_for_ships():
    ctx = context(river_world(nodes=[node(701, ResourceKind.FOOD, (59, 52)),
                                     node(702, ResourceKind.FOOD, (62, 52))]))
    assert Brain().economy.best_node(ctx, ctx.villagers[0], ResourceKind.FOOD) is None


def test_naval_manager_requires_known_water_before_planning_a_dock():
    manager = naval()
    ctx = context(river_world(nodes=[node(703, ResourceKind.FOOD, (54, 52))]))
    manager.step(ctx)
    assert ctx.client.named("build") == []
    assert not any(site[0] == BuildingKind.DOCK for site in ctx.world.sites)


def test_known_river_food_prompts_one_dock_and_no_duplicate_plan():
    manager = naval()
    board = Blackboard()
    ctx = context(river_world(), board)
    manager.step(ctx)
    docks = [args for args in ctx.client.named("build") if args[1] == BuildingKind.DOCK]
    assert len(docks) == 1
    assert any(site[0] == BuildingKind.DOCK for site in ctx.world.sites)
    later = context(river_world(tick=110), board)
    manager.step(later)
    assert later.client.named("build") == []


def test_dock_starts_with_one_fishing_ship_and_waits_for_its_order():
    manager = naval()
    dock = building(600, BuildingKind.DOCK, tile=(55, 50))
    board = Blackboard()
    world = river_world(docks=[dock])
    world.economy.population = 6_000
    ctx = context(world, board)
    manager.step(ctx)
    assert ctx.client.named("train") == [(600, UnitKind.FISHING_SHIP)]
    later = context(river_world(docks=[dock], tick=110), board)
    manager.step(later)
    assert later.client.named("train") == [], "the training verdict is still in flight"


def test_fishing_ship_works_river_food_and_galleys_stop_at_two():
    manager = naval()
    fishing = unit(200, UnitKind.FISHING_SHIP, tile=(59, 50))
    galleys = [unit(201 + i, UnitKind.GALLEY, tile=(59, 50 + i)) for i in range(2)]
    dock = building(600, BuildingKind.DOCK, tile=(55, 50))
    ctx = context(river_world(units=[fishing], docks=[dock]))
    manager.step(ctx)
    assert ctx.client.named("gather"), "the Fishing Ship needs a river fishing order"
    assert ctx.client.named("gather")[0][0] == fishing.id
    assert ctx.client.named("gather")[0][1] in {700, 701}
    assert ctx.client.named("train") == [(600, UnitKind.GALLEY)]
    full = context(river_world(units=[fishing] + galleys, docks=[dock]))
    manager.step(full)
    assert full.client.named("train") == []


def test_busy_dock_does_not_overqueue_the_bounded_fleet():
    dock = building(600, BuildingKind.DOCK, tile=(55, 50), queue=1)
    ctx = context(river_world(docks=[dock]))
    naval().step(ctx)
    assert ctx.client.named("train") == []


def test_castle_age_keeps_feudal_capabilities_and_galley_production():
    fishing = unit(200, UnitKind.FISHING_SHIP, tile=(59, 50))
    dock = building(600, BuildingKind.DOCK, tile=(55, 50))
    world = river_world(units=[fishing], docks=[dock])
    world.economy = economy(food=500, wood=1000, gold=500, population=5,
                            age=Age.CASTLE)
    ctx = context(world)
    assert ctx.feudal, "Castle Age retains access to Feudal military and research"
    naval().step(ctx)
    assert ctx.client.named("train") == [(600, UnitKind.GALLEY)]


def test_upgraded_war_galleys_still_fill_the_two_ship_fleet_limit():
    fishing = unit(200, UnitKind.FISHING_SHIP, tile=(59, 50))
    galleys = [unit(201 + i, UnitKind.WAR_GALLEY, tile=(59, 51 + i))
               for i in range(2)]
    dock = building(600, BuildingKind.DOCK, tile=(55, 50))
    world = river_world(units=[fishing] + galleys, docks=[dock])
    world.economy = economy(food=500, wood=1000, gold=500, population=5,
                            age=Age.CASTLE)
    ctx = context(world)
    naval().step(ctx)
    assert ctx.client.named("train") == [], "upgrading retains the small fleet cap"
