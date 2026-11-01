"""A failed job can leave an unemployed villager holding its last load."""

from aoeworld import Age, BuildingKind, CommandResult, ResourceKind, UnitTask

from aoe_ai import Brain
from aoe_ai.blackboard import Blackboard
from aoe_ai.config import Config
from aoe_ai.context import Context
from aoe_ai.play.constructor import Constructor
from aoe_ai.play.economy import EconomyManager
from aoe_ai.play.reseeding import queue_reseeds
from fakes import FakeClient, FakeWorld, base, building, decision, economy, node, order_result, unit


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
                                  age=Age.FEUDAL))
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
                                  age=Age.FEUDAL))
    ctx = Context(FakeClient(), decision(world), Blackboard(), Config())
    EconomyManager(Constructor()).work(ctx)
    assert ctx.client.calls == [], "loaded returns must complete before reallocation"
