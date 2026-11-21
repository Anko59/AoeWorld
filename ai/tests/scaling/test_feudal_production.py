"""A Feudal opening must fund its first army producer."""
from aoeworld import Age, BuildingKind, ResourceKind, Tech, UnitTask, building_cost

from aoe_ai.play import buildorder
from fakes import building
from test_kingdom import kingdom


def test_an_unbuilt_feudal_barracks_keeps_its_own_wood_budget():
    ctx = kingdom(villagers=28, centers=1, age=Age.FEUDAL, wood=150,
                  researched=[Tech.LOOM, Tech.FEUDAL_AGE])
    ctx.buildings[:] = [b for b in ctx.buildings if b.kind == BuildingKind.TOWN_CENTER]
    ctx.buildings.append(building(991, BuildingKind.BLACKSMITH))

    assert buildorder.wanted_buildings(ctx) == [(BuildingKind.BARRACKS, 1)]
    assert buildorder.castle_prerequisite_wood(ctx) == (
        building_cost(BuildingKind.BARRACKS)[1] + ctx.config.wood_reserve)


def test_a_small_feudal_settlement_can_raise_defenders_during_a_raid():
    from aoeworld import UnitKind
    from aoe_ai.play.military import MilitaryManager

    ctx = kingdom(villagers=11, centers=1, age=Age.FEUDAL, food=400, wood=400,
                  researched=[Tech.LOOM, Tech.FEUDAL_AGE])
    ctx.board.raid_until = ctx.tick + 100

    MilitaryManager().train(ctx)

    assert any(kind == UnitKind.SPEARMAN for _, kind in ctx.client.named("train"))


def test_feudal_barracks_can_start_at_its_exact_price():
    from aoe_ai.play.constructor import Constructor

    wood = building_cost(BuildingKind.BARRACKS)[1]
    ctx = kingdom(villagers=29, centers=1, age=Age.FEUDAL, wood=wood,
                  researched=[Tech.LOOM, Tech.FEUDAL_AGE])
    ctx.buildings[:] = [b for b in ctx.buildings if b.kind == BuildingKind.TOWN_CENTER]

    Constructor().production(ctx)

    assert any(kind == BuildingKind.BARRACKS for _, kind, _, _ in ctx.client.named("build"))


def test_a_feudal_barracks_trains_militia_when_local_wood_runs_out():
    from aoeworld import UnitKind
    from aoe_ai.play.military import MilitaryManager

    ctx = kingdom(villagers=28, centers=1, age=Age.FEUDAL, food=1000, wood=0, gold=350,
                  researched=[Tech.LOOM, Tech.FEUDAL_AGE])

    MilitaryManager().train(ctx)

    assert [kind for _, kind in ctx.client.named("train")] == [UnitKind.MILITIA]


def test_known_replacement_forest_keeps_camp_wood_between_gathering_trips():
    from aoe_ai import Brain
    from test_logic import play
    from test_worker_recovery import distant_wood_village

    world, brain = distant_wood_village(80), Brain()
    worker = world._units[0]
    brain.board.gather_sent[worker.id] = (world._nodes[0].id, world.tick)
    worker.tile = (40, 50)
    for task, carrying in ((UnitTask.MOVING, None),
                           (UnitTask.RETURNING, (ResourceKind.WOOD, 10))):
        worker.task, worker.carrying = task, carrying
        _, client = play(world, brain=brain)
        assert not client.named("train"), client.calls
        assert not [args for args in client.named("build") if args[1] == BuildingKind.LUMBER_CAMP]
        world.tick += 10

    worker.task, worker.tile, worker.carrying = UnitTask.MOVING, (21, 50), None
    world.economy.wood = 100
    _, client = play(world, brain=brain)
    assert [args for args in client.named("build") if args[1] == BuildingKind.LUMBER_CAMP]
