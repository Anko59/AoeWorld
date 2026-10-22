"""Decision logic against fake worlds: each rule of the player, in isolation."""

import aoeworld
from aoeworld import (
    Age, AnimalKind, BuildingKind, CommandResult, ResourceKind, Tech, UnitKind, UnitTask,
)

from aoe_ai import Brain, Config
from aoe_ai.geometry import dist, ring_points

from fakes import (
    FakeClient, FakeWorld, animal, base, building, decision, economy, node, order_result, unit,
)


def play(world, config=None, events=(), brain=None):
    brain = brain or Brain(config)
    client = FakeClient()
    brain.decide(client, decision(world, events))
    return brain, client


def test_the_town_center_queue_is_kept_full_while_food_lasts():
    units, buildings = base(5)
    world = FakeWorld(units, buildings, eco=economy(food=500, population=6, cap=15))
    _, client = play(world)
    trained = client.named("train")
    assert len(trained) == 2 and all(args == (500, UnitKind.VILLAGER) for args in trained)


def test_no_villager_is_queued_without_food_or_room():
    units, buildings = base(5)
    for eco in (economy(food=40), economy(food=500, population=10, cap=10)):
        _, client = play(FakeWorld(units, buildings, eco=eco))
        assert client.named("train") == []


def test_a_house_is_built_before_the_population_cap_is_hit():
    units, buildings = base(8)
    world = FakeWorld(units, buildings, eco=economy(wood=100, population=11, cap=15))
    brain, client = play(world)
    houses = [args for args in client.named("build") if args[1] == BuildingKind.HOUSE]
    assert len(houses) == 1
    # One house at a time while it is under way.
    brain2, client2 = play(world, brain=brain)
    assert [a for a in client2.named("build") if a[1] == BuildingKind.HOUSE] == []


def test_idle_villagers_follow_the_wood_and_food_shares():
    units, buildings = base(6)
    nodes = [node(1, ResourceKind.WOOD, (56, 52)), node(2, ResourceKind.FOOD, (44, 52))]
    world = FakeWorld(units, buildings, nodes=nodes, eco=economy(population=7, cap=20))
    _, client = play(world)
    gathered = {args[1] for args in client.named("gather")}
    assert gathered == {1, 2}, client.calls


def test_hunting_comes_before_foraging():
    units, buildings = base(4)
    world = FakeWorld(
        units, buildings,
        nodes=[node(2, ResourceKind.FOOD, (44, 52))],
        animals=[animal(900, (60, 55))],
        eco=economy(population=5, cap=20),
    )
    _, client = play(world)
    assert client.named("hunt") and client.named("hunt")[0][1] == 900


def test_far_wood_gets_a_lumber_camp_beside_it():
    units, buildings = base(4)
    world = FakeWorld(units, buildings, nodes=[node(1, ResourceKind.WOOD, (80, 50))],
                      eco=economy(population=5, cap=20))
    _, client = play(world)
    camps = [a for a in client.named("build") if a[1] == BuildingKind.LUMBER_CAMP]
    assert camps, client.calls
    assert any(kind == BuildingKind.LUMBER_CAMP and abs(x - 80) <= 8 for kind, x, y in world.sites)


def test_farms_start_with_a_mill_when_nothing_else_feeds_us():
    units, buildings = base(4)
    world = FakeWorld(units, buildings, eco=economy(wood=300, population=5, cap=20))
    brain, client = play(world)
    assert [a for a in client.named("build") if a[1] == BuildingKind.MILL]
    # With a mill standing, the next idle villager builds a farm, then works it.
    buildings.append(building(600, BuildingKind.MILL, tile=(54, 50)))
    brain.board.plans.clear()
    brain.board.builders.clear()
    _, client = play(FakeWorld(units, buildings, eco=economy(wood=300, population=5, cap=20), tick=400), brain=brain)
    assert [a for a in client.named("build") if a[1] == BuildingKind.FARM]
    buildings.append(building(700, BuildingKind.FARM, tile=(56, 50)))
    brain.board.plans.clear()
    _, client = play(FakeWorld(units, buildings, eco=economy(wood=300, population=5, cap=20), tick=700), brain=brain)
    assert client.named("farm") and client.named("farm")[0][1] == 700


def test_a_refused_resource_is_never_tried_again():
    units, buildings = base(3)
    nodes = [node(1, ResourceKind.WOOD, (56, 52)), node(3, ResourceKind.WOOD, (58, 52))]
    brain, client = play(FakeWorld(units, buildings, nodes=nodes, eco=economy(population=4, cap=20)))
    first = client.named("gather")[0][1]
    sequence = next(i + 1 for i, (n, a) in enumerate(client.calls) if n == "gather" and a[1] == first)
    _, client2 = play(
        FakeWorld(units, buildings, nodes=nodes, eco=economy(population=4, cap=20)),
        brain=brain,
        events=[order_result(sequence, CommandResult.REJECTED_INVALID_RESOURCE)],
    )
    assert first in brain.board.bad_resources
    assert all(args[1] != first for args in client2.named("gather"))


def test_loom_then_feudal_age_with_the_queue_drained_first():
    units, buildings = base(12)
    world = FakeWorld(units, buildings, eco=economy(gold=100, population=13, cap=25))
    _, client = play(world)
    assert client.named("research") == [(500, Tech.LOOM)]

    units, buildings = base(22)
    buildings += [building(600, BuildingKind.MILL, tile=(54, 50)),
                  building(601, BuildingKind.LUMBER_CAMP, tile=(58, 50))]
    eco = economy(food=600, gold=0, population=23, cap=30, researched=[Tech.LOOM])
    busy = [b if b.kind != BuildingKind.TOWN_CENTER else building(500, b.kind, b.center, queue=1) for b in buildings]
    _, client = play(FakeWorld(units, busy, eco=eco))
    assert client.named("research") == [], "the queue must drain first"
    assert client.named("train") == [], "and nothing is queued behind it"
    _, client = play(FakeWorld(units, buildings, eco=eco))
    assert client.named("research") == [(500, Tech.FEUDAL_AGE)]


def test_the_army_mix_trains_the_least_represented_unit():
    units, buildings = base(20)
    buildings += [building(600, BuildingKind.BARRACKS, tile=(60, 50)),
                  building(601, BuildingKind.ARCHERY_RANGE, tile=(64, 50))]
    army = [unit(200 + i, UnitKind.SPEARMAN, tile=(55, 55), task=UnitTask.MOVING) for i in range(6)]
    eco = economy(food=900, wood=900, gold=500, population=22, cap=40, age=Age.FEUDAL)
    world = FakeWorld(units + army, buildings, eco=eco)
    _, client = play(world)
    trained = {args[0]: args[1] for args in client.named("train") if args[0] in (600, 601)}
    assert trained[600] == UnitKind.SPEARMAN or trained[600] in (UnitKind.MILITIA,)
    assert trained[601] in (UnitKind.ARCHER, UnitKind.SKIRMISHER)


def test_the_scout_checks_home_then_rides_the_map_circle_far_side_first():
    units, buildings = base(4)
    brain = Brain()
    visited = []
    for _ in range(14):
        _, client = play(FakeWorld(units, buildings, eco=economy(food=0, wood=0, population=5, cap=20)), brain=brain)
        moves = [a for a in client.named("move") if a[0] == 100]
        assert len(moves) == 1
        visited.append((moves[0][1], moves[0][2]))
    home = (52, 52)
    assert all(abs(dist(p, home) - 18) < 2 for p in visited[:6]), visited
    far = visited[9]  # first point of the map circle: the mirror of home
    assert dist(far, home) > 150 and len(set(visited)) == len(visited), visited


def test_a_scout_that_is_busy_is_left_alone():
    units, buildings = base(4)
    units[-1].idle = False
    _, client = play(FakeWorld(units, buildings, eco=economy(food=0, wood=0, population=5, cap=20)))
    assert [a for a in client.named("move") if a[0] == 100] == []


def test_a_strong_army_attacks_the_nearest_known_enemy_building():
    units, buildings = base(10)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(14)]
    far = building(900, BuildingKind.HOUSE, tile=(250, 250), mine=False, visible=False)
    near = building(901, BuildingKind.BARRACKS, tile=(150, 120), mine=False, visible=False)
    world = FakeWorld(units + army, buildings, enemy_buildings=[far, near], eco=economy(population=25, cap=40))
    brain, client = play(world)
    assert brain.board.attacking and brain.board.attack_target == 901
    moves = [a for a in client.named("move") if a[0] >= 300]
    assert len(moves) == 14 and all(dist((a[1], a[2]), near.center) <= 6 for a in moves)
    # Once the target is in sight the army hits it instead of walking.
    near.visible = True
    _, client = play(FakeWorld(units + army, buildings, enemy_buildings=[far, near]), brain=brain)
    assert len([a for a in client.named("attack") if a[1] == 901]) == 14


def test_a_small_army_keeps_massing_and_a_failing_attack_retreats():
    units, buildings = base(10)
    few = [unit(300 + i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(5)]
    brain, client = play(FakeWorld(units + few, buildings))
    assert not brain.board.attacking
    brain.board.attacking = True
    brain.board.attack_launch_size = 14
    play(FakeWorld(units + few, buildings), brain=brain)
    assert not brain.board.attacking and brain.board.stats["retreats"] == 1


def test_enemies_at_the_door_pull_the_army_into_defence():
    units, buildings = base(10)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(56, 56)) for i in range(4)]
    raider = unit(800, UnitKind.SPEARMAN, tile=(60, 56), mine=False)
    _, client = play(FakeWorld(units + army, buildings, enemies=[raider]))
    assert {a[1] for a in client.named("attack")} == {800}
    assert len(client.named("attack")) == 4


def test_a_refused_spearman_is_replaced_by_militia_until_the_age_changes():
    units, buildings = base(20)
    buildings.append(building(600, BuildingKind.BARRACKS, tile=(60, 50)))
    eco = economy(food=900, wood=900, gold=500, population=22, cap=40, age=Age.FEUDAL)
    brain, client = play(FakeWorld(units, buildings, eco=eco))
    first = next(args for args in client.named("train") if args[0] == 600)
    assert first[1] == UnitKind.SPEARMAN
    sequence = next(i + 1 for i, (n, a) in enumerate(client.calls) if n == "train" and a[0] == 600)
    refusal = order_result(sequence, CommandResult.REJECTED_REQUIRES_FEUDAL_AGE)
    _, client = play(FakeWorld(units, buildings, eco=eco), brain=brain, events=[refusal])
    assert next(args for args in client.named("train") if args[0] == 600)[1] == UnitKind.MILITIA
    advanced = type("E", (), {"kind": "age_advanced"})()
    _, client = play(FakeWorld(units, buildings, eco=eco), brain=brain, events=[advanced])
    assert next(args for args in client.named("train") if args[0] == 600)[1] == UnitKind.SPEARMAN


def test_no_unit_is_trained_in_the_dark_age():
    units, buildings = base(20)
    buildings.append(building(600, BuildingKind.BARRACKS, tile=(60, 50)))
    eco = economy(food=900, wood=900, gold=500, population=22, cap=40)
    _, client = play(FakeWorld(units, buildings, eco=eco))
    assert [a for a in client.named("train") if a[0] == 600] == []


def test_the_barracks_is_raised_while_feudal_age_researches():
    units, buildings = base(22)
    buildings[0] = building(500, BuildingKind.TOWN_CENTER, tile=(50, 50), researching=Tech.FEUDAL_AGE)
    buildings += [building(600, BuildingKind.MILL, tile=(54, 50)),
                  building(601, BuildingKind.LUMBER_CAMP, tile=(58, 50))]
    eco = economy(food=100, wood=400, population=23, cap=40, researched=[Tech.LOOM])
    _, client = play(FakeWorld(units, buildings, eco=eco))
    assert [a for a in client.named("build") if a[1] == BuildingKind.BARRACKS]


def test_feudal_age_adds_a_range_then_a_blacksmith_and_economy_upgrades():
    units, buildings = base(24)
    buildings += [building(600, BuildingKind.BARRACKS, tile=(60, 50)),
                  building(601, BuildingKind.LUMBER_CAMP, tile=(58, 56))]
    eco = economy(food=900, wood=900, gold=300, population=25, cap=60, age=Age.FEUDAL, researched=[Tech.LOOM])
    brain, client = play(FakeWorld(units, buildings, eco=eco))
    kinds = [a[1] for a in client.named("build")]
    assert BuildingKind.ARCHERY_RANGE in kinds
    assert client.named("research") == [(601, Tech.DOUBLE_BIT_AXE)]
    # The Blacksmith follows once the range stands (one production building at a time).
    brain.board.plans.clear()
    buildings.append(building(602, BuildingKind.ARCHERY_RANGE, tile=(64, 50)))
    brain.board.cooldowns.clear()
    _, client = play(FakeWorld(units, buildings, eco=eco), brain=brain)
    assert BuildingKind.BLACKSMITH in [a[1] for a in client.named("build")]


def test_sheep_come_before_berries_and_a_villager_that_stays_idle_is_not_spammed():
    units, buildings = base(3)
    nodes = [node(2, ResourceKind.FOOD, (44, 52))]
    sheep = animal(900, (46, 50), kind=AnimalKind.SHEEP)
    world = FakeWorld(units, buildings, nodes=nodes, animals=[sheep], eco=economy(population=4, cap=20))
    brain, client = play(world)
    hunts = client.named("hunt")
    assert hunts and hunts[0][1] == 900
    # The same idle villagers at the next decision (one second later) get no new order.
    world.tick += 20
    _, client = play(world, brain=brain)
    assert client.named("hunt") == [] and client.named("gather") == []


def test_outnumbered_at_home_the_villagers_beside_a_raider_walk_to_the_town_center():
    units, buildings = base(6)
    raiders = [unit(800 + i, UnitKind.SPEARMAN, tile=(51, 51 + i), mine=False) for i in range(3)]
    brain, client = play(FakeWorld(units, buildings, enemies=raiders))
    home = (52, 52)
    pulled = [a for a in client.named("move") if (a[1], a[2]) == (home[0] + 4, home[1] + 4)]
    assert pulled and brain.board.stats["villagers_pulled_back"] == len(pulled)


def test_the_army_walks_to_a_tile_beside_the_enemy_building_not_onto_it():
    units, buildings = base(10)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(12)]
    target = building(901, BuildingKind.BARRACKS, tile=(150, 120), mine=False, visible=False)
    _, client = play(FakeWorld(units + army, buildings, enemy_buildings=[target]))
    moves = [a for a in client.named("move") if a[0] >= 300]
    assert moves and all(dist((a[1], a[2]), target.center) in range(2, 6) for a in moves)
    assert all((a[1], a[2]) != target.center for a in moves)


def test_a_refused_destination_is_not_chosen_again_for_the_rally_point():
    units, buildings = base(20)
    army = [unit(300, UnitKind.SPEARMAN, tile=(50, 50))]
    eco = economy(population=22, cap=40, age=Age.FEUDAL)
    brain, client = play(FakeWorld(units + army, buildings, eco=eco))
    first = next(a for a in client.named("move") if a[0] == 300)
    sequence = next(i + 1 for i, (n, a) in enumerate(client.calls) if n == "move" and a[0] == 300)
    refusal = order_result(sequence, CommandResult.REJECTED_INVALID_DESTINATION)
    _, client = play(FakeWorld(units + army, buildings, eco=eco), brain=brain, events=[refusal])
    second = next(a for a in client.named("move") if a[0] == 300)
    assert (first[1], first[2]) != (second[1], second[2])
