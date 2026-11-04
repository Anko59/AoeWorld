"""A failed job can leave an unemployed villager holding its last load."""

from aoeworld import Age, BuildingKind, CommandResult, ResourceKind, Tech, UnitTask

from aoe_ai import Brain
from aoe_ai.blackboard import Blackboard
from aoe_ai.config import Config
from aoe_ai.context import Context
from aoe_ai.play.constructor import Constructor
from aoe_ai.play.economy import EconomyManager
from aoe_ai.play.reseeding import queue_reseeds
from fakes import FakeClient, FakeWorld, animal, base, building, decision, economy, node, order_result, unit
from test_logic import play


def advancing_village(wood, barracks=None):
    units, buildings = base(21)
    buildings[0].researching = Tech.FEUDAL_AGE
    buildings += [building(901, BuildingKind.MILL), building(902, BuildingKind.LUMBER_CAMP)]
    if barracks is not None:
        buildings.append(barracks)
    return FakeWorld(units, buildings, nodes=[node(800, ResourceKind.WOOD, (55, 48))],
                     eco=economy(food=100, wood=wood, population=22, cap=50,
                                 researched=[Tech.LOOM]))


def test_farms_cannot_spend_the_first_barracks_budget_while_feudal_researches():
    for wood in (100, 200, 225):
        _, client = play(advancing_village(wood))
        kinds = [args[1] for args in client.named("build")]
        assert BuildingKind.FARM not in kinds, client.calls
        assert (BuildingKind.BARRACKS in kinds) == (wood == 225), client.calls
        assert client.named("gather"), "workers without affordable food should cut wood"


def test_first_barracks_budget_accumulates_before_the_age_is_clicked():
    world = advancing_village(100)
    world._buildings[0].researching = None
    _, client = play(world)
    assert not [args for args in client.named("build") if args[1] == BuildingKind.FARM]
    assert client.named("gather")


def test_paid_barracks_foundation_or_plan_does_not_keep_reserving_its_wood():
    from aoe_ai.blackboard import Plan
    for planned in (False, True):
        brain = Brain()
        barracks = building(903, BuildingKind.BARRACKS, complete=False)
        world = advancing_village(110, None if planned else barracks)
        if planned:
            brain.board.plans.append(Plan(BuildingKind.BARRACKS, (60, 60), world.tick))
        _, client = play(world, brain=brain)
        assert [args for args in client.named("build") if args[1] == BuildingKind.FARM]
        assert not [args for args in client.named("build") if args[1] == BuildingKind.BARRACKS]


def test_prepaid_reseeds_cannot_drain_the_missing_first_barracks_budget():
    world = advancing_village(200)
    world._buildings.append(building(904, BuildingKind.FARM))
    _, client = play(world)
    assert not client.named("farm_reseed")


def test_surplus_wood_can_fund_a_farm_after_the_first_barracks_is_ordered():
    _, client = play(advancing_village(285))
    kinds = [args[1] for args in client.named("build")]
    assert kinds[:2] == [BuildingKind.BARRACKS, BuildingKind.FARM], client.calls


def test_barracks_saving_still_uses_berries_and_existing_farms_without_spending_wood():
    for berries in (False, True):
        world = advancing_village(100)
        if berries:
            world._nodes.append(node(801, ResourceKind.FOOD, (51, 48)))
        else:
            world._buildings.append(building(904, BuildingKind.FARM))
        _, client = play(world)
        assert ((any(args[1] == 801 for args in client.named("gather"))) if berries
                else bool(client.named("farm")))
        assert not [args for args in client.named("build") if args[1] == BuildingKind.FARM]


def test_unreachable_hunt_falls_back_to_wood_without_spending_the_barracks_budget():
    world = advancing_village(0)
    world._animals.append(animal(905, (51, 48)))
    brain, client = play(world)
    sequence = next(i + 1 for i, (name, _) in enumerate(client.calls) if name == "hunt")
    gathering = {args[0] for args in client.named("gather")}
    for worker in world._units:
        if worker.id in gathering:
            worker.task, worker.idle, worker.gathering = UnitTask.GATHERING, False, ResourceKind.WOOD
    world.tick += 100
    world.economy.wood = 200
    _, client = play(world, brain=brain,
                     events=[order_result(sequence, CommandResult.REJECTED_UNREACHABLE)])
    assert not client.named("hunt")
    assert not [args for args in client.named("build") if args[1] == BuildingKind.FARM]
    assert client.named("gather")


def distant_wood_village(wood):
    units, buildings = base(28)
    for worker in units[:-1]:
        worker.task, worker.idle, worker.gathering = UnitTask.GATHERING, False, ResourceKind.FOOD
    units[0].tile = (20, 50)
    units[0].gathering = ResourceKind.WOOD
    buildings += [building(901, BuildingKind.MILL), building(902, BuildingKind.LUMBER_CAMP),
                  building(903, BuildingKind.BARRACKS), building(904, BuildingKind.ARCHERY_RANGE),
                  building(905, BuildingKind.BLACKSMITH)]
    return FakeWorld(units, buildings, nodes=[node(800, ResourceKind.WOOD, (20, 50))],
                     eco=economy(food=900, wood=wood, gold=600, population=29, cap=100,
                                 age=Age.FEUDAL, researched=[Tech.LOOM, Tech.FEUDAL_AGE]))


def test_distant_woodcutters_save_for_a_camp_before_small_deliveries_feed_army_queues():
    for wood in (80, 100):
        _, client = play(distant_wood_village(wood))
        assert not client.named("train"), client.calls
        camps = [args for args in client.named("build") if args[1] == BuildingKind.LUMBER_CAMP]
        assert bool(camps) == (wood == 100), client.calls
        if camps:
            assert camps[0][2:] == (23, 53), "place beside the new forest"


def test_an_idle_worker_cannot_turn_camp_saving_into_a_failed_site_cooldown():
    world, brain = distant_wood_village(80), Brain()
    world._units.append(unit(700))
    for wood in (80, 90, 100):
        world.economy.wood = wood
        _, client = play(world, brain=brain)
        assert not client.named("train"), client.calls
        camps = [args for args in client.named("build") if args[1] == BuildingKind.LUMBER_CAMP]
        assert bool(camps) == (wood == 100), client.calls
        world.tick += 10


def test_a_paid_distant_camp_does_not_reserve_its_cost_again():
    from aoe_ai.blackboard import Plan
    for planned in (False, True):
        world, brain = distant_wood_village(80), Brain()
        if planned:
            brain.board.plans.append(Plan(BuildingKind.LUMBER_CAMP, (20, 50), world.tick))
        else:
            world._buildings.append(building(906, BuildingKind.LUMBER_CAMP, tile=(20, 50), complete=False))
        _, client = play(world, brain=brain)
        assert client.named("train")
        assert not [args for args in client.named("build") if args[1] == BuildingKind.LUMBER_CAMP]


def test_a_blocked_camp_site_does_not_freeze_military_production():
    world = distant_wood_village(100)
    world.find_build_site = lambda *args: None
    _, client = play(world)
    assert client.named("train")


def test_a_loaded_wood_return_is_not_pulled_off_its_delivery_to_build_a_camp():
    world = distant_wood_village(100)
    world._units[0].task = UnitTask.RETURNING
    world._units[0].carrying = (ResourceKind.WOOD, 10)
    _, client = play(world)
    assert client.named("train")
    assert not [args for args in client.named("build") if args[1] == BuildingKind.LUMBER_CAMP]


def test_ai_prepays_reseeds_only_with_farms_and_spare_wood():
    units, buildings = base(1)
    mill = building(901, BuildingKind.MILL)
    mill.farm_reseeds = 0
    farm = building(902, BuildingKind.FARM, tile=(55, 55))
    world = FakeWorld(units, buildings + [mill, farm])
    ctx = Context(FakeClient(), decision(world), Blackboard(), Config())
    queue_reseeds(ctx)
    assert ctx.client.named("farm_reseed") == [(mill.id, True)]
    assert ctx.spent[1] == 60
    queue_reseeds(ctx)
    assert len(ctx.client.named("farm_reseed")) == 1, "wait for authoritative count"
    for wood, farms, queued in [(59, True, 0), (200, False, 0), (200, True, 4)]:
        world.economy.wood = wood
        world._buildings = buildings + [mill] + ([farm] if farms else [])
        mill.farm_reseeds = queued
        ctx = Context(FakeClient(), decision(world), Blackboard(), Config())
        queue_reseeds(ctx)
        assert ctx.client.calls == []


def worker_context(gathering=None):
    units, buildings = base(1)
    worker = units[0]
    worker.task, worker.idle = UnitTask.CARRYING, False
    worker.carrying = (ResourceKind.FOOD, 7)
    worker.gathering = gathering
    food = node(900, ResourceKind.FOOD, (52, 48))
    world = FakeWorld(units, buildings, nodes=[food])
    client = FakeClient()
    ctx = Context(client, decision(world), Blackboard(), Config())
    return ctx, worker, food


def test_worker_with_a_load_and_no_gather_job_returns_to_work():
    ctx, worker, food = worker_context()
    EconomyManager(Constructor()).work(ctx)
    assert ctx.client.named("gather") == [(worker.id, food.id)]


def test_worker_waiting_for_a_dropoff_keeps_its_live_gather_job():
    ctx, _, _ = worker_context(ResourceKind.FOOD)
    EconomyManager(Constructor()).work(ctx)
    assert ctx.client.calls == []


def test_finished_builder_with_a_load_returns_to_work():
    ctx, worker, food = worker_context()
    ctx.board.builders[worker.id] = ("house", (50, 50))
    EconomyManager(Constructor()).work(ctx)
    assert ctx.client.named("gather") == [(worker.id, food.id)]
    assert worker.id not in ctx.board.builders


def test_automatic_farmers_reserve_their_farms_before_idle_workers_are_assigned():
    units, buildings = base(1)
    farm = building(900, BuildingKind.FARM, tile=(55, 55))
    farmer = unit(800, tile=farm.center, task=UnitTask.FARMING, gathering=ResourceKind.FOOD)
    world = FakeWorld(units + [farmer], buildings + [farm])
    ctx = Context(FakeClient(), decision(world), Blackboard(), Config())
    EconomyManager(Constructor()).work(ctx)
    assert ctx.board.farmers[farm.id] == farmer.id
    assert all(args[1] != farm.id for args in ctx.client.named("farm"))


def test_a_refused_farm_does_not_trap_workers_in_a_retry_loop():
    units, buildings = base(1)
    busy = building(900, BuildingKind.FARM, tile=(52, 52))
    free = building(901, BuildingKind.FARM, tile=(60, 60))
    world = FakeWorld(units, buildings + [busy, free])
    brain, client = Brain(), FakeClient()
    ctx = Context(client, decision(world), brain.board, brain.config)
    sequence = ctx.order("farm", units[0].id, busy.id)
    ctx.events = [order_result(sequence, CommandResult.REJECTED_FARM_UNAVAILABLE)]
    brain.absorb(ctx)
    client.calls.clear()
    EconomyManager(Constructor()).send_to_farm(ctx, units[0])
    assert client.named("farm") == [(units[0].id, free.id)]


def test_a_refusal_for_another_worker_preserves_the_known_farmer():
    units, buildings = base(1)
    farm = building(900, BuildingKind.FARM, tile=(52, 52))
    farmer = unit(800, tile=farm.center, task=UnitTask.FARMING, gathering=ResourceKind.FOOD)
    world = FakeWorld(units + [farmer], buildings + [farm])
    brain = Brain()
    ctx = Context(FakeClient(), decision(world), brain.board, brain.config)
    ctx.board.farmers[farm.id] = farmer.id
    sequence = ctx.order("farm", units[0].id, farm.id)
    ctx.events = [order_result(sequence, CommandResult.REJECTED_FARM_UNAVAILABLE)]
    brain.absorb(ctx)
    assert ctx.board.farmers[farm.id] == farmer.id


def test_constructor_preserves_an_automatic_farmer_even_before_it_is_remembered():
    units, buildings = base(1)
    farmer = unit(800, tile=(52, 52), task=UnitTask.FARMING, gathering=ResourceKind.FOOD)
    ctx = Context(FakeClient(), decision(FakeWorld(units + [farmer], buildings)), Blackboard(), Config())
    assert Constructor().builder_near(ctx, farmer.tile).id == units[0].id


def test_a_worker_without_a_farm_job_releases_its_old_plot():
    ctx, worker, _ = worker_context()
    farm = building(900, BuildingKind.FARM, tile=(55, 55))
    ctx.world._buildings.append(farm)
    ctx.buildings.append(farm)
    ctx.board.farmers[farm.id] = worker.id
    EconomyManager(Constructor()).forget_farmers(ctx)
    assert farm.id not in ctx.board.farmers


def test_live_food_jobs_rebalance_to_wood_without_interrupting_loaded_returns():
    units, buildings = base(28)
    workers = units[:-1]
    for worker in workers:
        worker.task, worker.idle = UnitTask.GATHERING, False
        worker.gathering = ResourceKind.FOOD
    workers[-1].gathering = ResourceKind.WOOD
    workers[0].task, workers[0].moving = UnitTask.RETURNING, True
    workers[0].carrying = (ResourceKind.FOOD, 10)
    wood = node(900, ResourceKind.WOOD, (53, 53))
    food = node(901, ResourceKind.FOOD, (52, 48))
    world = FakeWorld(units, buildings, nodes=[wood, food],
                      eco=economy(food=273, wood=14, gold=580, population=36,
                                  cap=50, age=Age.FEUDAL))
    ctx = Context(FakeClient(), decision(world), Blackboard(), Config())
    # Seed 1 held 27 live food jobs and one wood job for eight minutes. Idle-only
    # allocation never repaired that shortage, leaving its Feudal army at seven.
    EconomyManager(Constructor()).work(ctx)
    reassigned = ctx.client.named("gather")
    assert len(reassigned) == 1, "rebalance one worker per decision"
    assert reassigned[0][1] == wood.id
    assert reassigned[0][0] not in (workers[0].id, workers[-1].id)


def test_rebalancing_waits_for_excess_food_workers_to_finish_their_loads():
    units, buildings = base(28)
    for worker in units[:-1]:
        worker.task, worker.idle, worker.moving = UnitTask.RETURNING, False, True
        worker.gathering = ResourceKind.FOOD
        worker.carrying = (ResourceKind.FOOD, 10)
    world = FakeWorld(units, buildings, nodes=[node(900, ResourceKind.WOOD, (53, 53))],
                      eco=economy(food=273, wood=14, gold=580, population=36,
                                  cap=50, age=Age.FEUDAL))
    ctx = Context(FakeClient(), decision(world), Blackboard(), Config())
    EconomyManager(Constructor()).work(ctx)
    assert ctx.client.calls == [], "loaded returns must complete before reallocation"
