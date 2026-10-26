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


def army_moves(client):
    """Expand the current grouped wire command for per-member destination checks."""
    return client.named("move") + [(unit, args[1], args[2])
        for args in client.named("formation_move") for unit in args[0]]


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
        # The scout arrives before it is idle again.
        scout = units[-1]
        scout.x, scout.y = visited[-1]
        scout.tile = visited[-1]
    home = (52, 52)
    assert all(abs(dist(p, home) - 18) < 2 for p in visited[:6]), visited
    far = visited[9]  # first point of the map circle: the mirror of home
    assert dist(far, home) > 150 and len(set(visited)) == len(visited), visited


def test_a_scout_that_cannot_reach_a_far_waypoint_drops_the_far_ones():
    # The fake scout never moves: its ride to the first map-circle point ends
    # far short of it, so the rest of the circle is dropped for nearer rings.
    units, buildings = base(4)
    brain = Brain()
    visited = []
    for _ in range(14):
        _, client = play(FakeWorld(units, buildings, eco=economy(food=0, wood=0, population=5, cap=20)), brain=brain)
        moves = [a for a in client.named("move") if a[0] == 100]
        visited.append((moves[0][1], moves[0][2]))
    home = (52, 52)
    far = [i for i, p in enumerate(visited) if dist(p, home) > 150]
    assert far, visited
    assert all(dist(p, home) <= 110 for p in visited[far[0] + 1 :]), visited


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
    brain, client = play(world, config=Config(march_hop_tiles=10_000))
    assert brain.board.attacking and brain.board.attack_target == 901
    moves = [a for a in army_moves(client) if a[0] >= 300]
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
    # Finish the second distinct Feudal kind before duplicating production.
    brain.board.plans.clear()
    buildings.append(building(602, BuildingKind.ARCHERY_RANGE, tile=(64, 50)))
    brain.board.cooldowns.clear()
    _, client = play(FakeWorld(units, buildings, eco=eco), brain=brain)
    assert BuildingKind.BLACKSMITH in [a[1] for a in client.named("build")]
    brain.board.plans.clear()
    buildings.append(building(603, BuildingKind.BLACKSMITH, tile=(68, 50)))
    brain.board.cooldowns.clear()
    _, client = play(FakeWorld(units, buildings, eco=eco), brain=brain)
    assert BuildingKind.BARRACKS in [a[1] for a in client.named("build")]


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
    lone = [unit(900, UnitKind.SCOUT, tile=(51, 51), mine=False)]
    _, calm = play(FakeWorld(units, buildings, enemies=lone))
    assert [a for a in calm.named('move') if (a[1], a[2]) == (56, 56)] == []
    brain, client = play(FakeWorld(units, buildings, enemies=raiders))
    home = (52, 52)
    pulled = [a for a in client.named("move") if (a[1], a[2]) == (home[0] + 4, home[1] + 4)]
    assert pulled and brain.board.stats["villagers_pulled_back"] == len(pulled)


def test_the_army_walks_to_a_tile_beside_the_enemy_building_not_onto_it():
    units, buildings = base(10)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(12)]
    target = building(901, BuildingKind.BARRACKS, tile=(150, 120), mine=False, visible=False)
    _, client = play(FakeWorld(units + army, buildings, enemy_buildings=[target]), config=Config(march_hop_tiles=10_000))
    moves = [a for a in army_moves(client) if a[0] >= 300]
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


def test_a_world_limit_refusal_keeps_the_site_and_pauses_building():
    units, buildings = base(20)
    eco = economy(food=900, wood=900, gold=500, population=20, cap=25)
    brain, client = play(FakeWorld(units, buildings, eco=eco))
    builds = [i + 1 for i, (n, _) in enumerate(client.calls) if n == "build"]
    assert builds
    site = next(a for a in client.named("build"))
    refusal = order_result(builds[0], CommandResult.REJECTED_WORLD_LIMIT)
    _, client = play(FakeWorld(units, buildings, eco=eco), brain=brain, events=[refusal])
    assert (str(site[1]), site[2], site[3]) not in brain.board.bad_sites
    assert client.named("build") == []


def test_a_villager_that_cannot_reach_its_node_is_sent_to_another():
    # One villager and two wood nodes; the near one cannot be reached: the
    # villager is idle again a few seconds after the order.
    units = [unit(1, tile=(50, 56))]
    buildings = [building(500, BuildingKind.TOWN_CENTER, tile=(50, 50)),
                 building(501, BuildingKind.LUMBER_CAMP, tile=(60, 60))]
    nodes = [node(700, ResourceKind.WOOD, (58, 58)), node(701, ResourceKind.WOOD, (64, 64))]
    eco = economy(food=0, wood=0, gold=0, stone=0, population=1, cap=10)
    brain = Brain()
    targets = []
    for tick in (100, 200, 300):
        _, client = play(FakeWorld(units, buildings, eco=eco, nodes=nodes, tick=tick), brain=brain)
        targets += [a[1] for a in client.named("gather") if a[0] == 1]
    assert targets[0] == 700, targets
    assert targets[1:] and all(t == 701 for t in targets[1:]), targets


def test_an_army_marches_to_a_far_target_in_short_hops():
    units, buildings = base(10)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(14)]
    far = building(900, BuildingKind.HOUSE, tile=(250, 250), mine=False, visible=False)
    _, client = play(FakeWorld(units + army, buildings, enemy_buildings=[far], eco=economy(population=25, cap=40)))
    moves = [a for a in army_moves(client) if a[0] >= 300]
    assert len(moves) == 14 and len({(a[1], a[2]) for a in moves}) == 1, moves
    stop = (moves[0][1], moves[0][2])
    assert abs(dist(stop, (60, 60)) - 48) <= 2 and dist(stop, far.center) < dist((60, 60), far.center), moves


def test_an_army_that_gets_no_nearer_gives_its_target_up_and_waits_between_orders():
    units, buildings = base(10)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(14)]
    far = building(900, BuildingKind.HOUSE, tile=(250, 250), mine=False, visible=False)
    eco = economy(population=25, cap=40)
    brain, client = play(FakeWorld(units + army, buildings, enemy_buildings=[far], eco=eco, tick=100))
    assert len([a for a in army_moves(client) if a[0] >= 300]) == 14
    # Idle again a second later (stopped short): not re-sent at once.
    _, client = play(FakeWorld(units + army, buildings, enemy_buildings=[far], eco=eco, tick=120), brain=brain)
    assert [a for a in army_moves(client) if a[0] >= 300] == []
    # A minute on and no nearer: the building is given up.
    play(FakeWorld(units + army, buildings, enemy_buildings=[far], eco=eco, tick=100 + 61 * 20), brain=brain)
    assert 900 in brain.board.cleared


def test_town_center_rallies_to_food_then_wood_and_keeps_the_same_target():
    units, buildings = base(5)
    food = node(2, ResourceKind.FOOD, (44, 52))
    wood = node(1, ResourceKind.WOOD, (56, 52))
    for nodes, expected in [([food, wood], food.id), ([wood], wood.id)]:
        world = FakeWorld(units, buildings, nodes=nodes, eco=economy(food=500, population=6, cap=15))
        _, client = play(world)
        assert client.named("set_rally_resource") == [(500, expected)]
        buildings[0].rally_resource = expected
        _, accepted_client = play(world)
        assert accepted_client.named("set_rally_resource") == []
        buildings[0].rally_resource = None



def test_army_chooses_a_building_on_its_own_land_component():
    units, buildings = base(12)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(55, 55)) for i in range(14)]
    across = building(900, BuildingKind.HOUSE, tile=(70, 55), mine=False, visible=True)
    reachable = building(901, BuildingKind.HOUSE, tile=(55, 90), mine=False, visible=True)
    world = FakeWorld(units + army, buildings, enemy_buildings=[across, reachable],
                      eco=economy(population=27, cap=40))
    world.land_route = lambda x, y, tx, ty: None if tx >= 65 else [(x, y), (tx, ty)]
    _, client = play(world)
    attacks = [a for a in client.named("attack") if a[0] >= 300]
    assert attacks and all(a[1] == 901 for a in attacks)


def test_visible_enemies_across_a_river_do_not_pin_the_army():
    units, buildings = base(12)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(55, 55)) for i in range(14)]
    foe = unit(900, UnitKind.SPEARMAN, tile=(58, 55), mine=False)
    world = FakeWorld(units + army, buildings, enemies=[foe],
                      eco=economy(population=27, cap=40))
    world.land_route = lambda x, y, tx, ty: None if tx >= 58 else [(x, y), (tx, ty)]
    _, client = play(world)
    assert client.named("attack") == []


def test_each_straggler_marches_in_a_short_land_hop():
    units, buildings = base(12)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(60 if i else 5, 60)) for i in range(14)]
    far = building(900, BuildingKind.HOUSE, tile=(250, 250), mine=False, visible=False)
    _, client = play(FakeWorld(units + army, buildings, enemy_buildings=[far],
                               eco=economy(population=27, cap=40)))
    origins = {u.id: u.tile for u in army}
    moves = [a for a in army_moves(client) if a[0] in origins]
    assert moves and all(dist(origins[a[0]], (a[1], a[2])) <= 48 for a in moves)


def test_a_visible_distant_building_still_uses_short_march_hops():
    units, buildings = base(12)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(14)]
    far = building(900, BuildingKind.HOUSE, tile=(250, 250), mine=False, visible=True)
    _, client = play(FakeWorld(units + army, buildings, enemy_buildings=[far],
                               eco=economy(population=27, cap=40)))
    assert [a for a in client.named("attack") if a[0] >= 300] == []
    moves = [a for a in army_moves(client) if a[0] >= 300]
    assert moves and all(dist((60, 60), (a[1], a[2])) <= 48 for a in moves)


def test_defence_does_not_order_soldiers_across_a_river_to_home():
    units, buildings = base(12)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(80, 55)) for i in range(14)]
    foe = unit(900, UnitKind.SPEARMAN, tile=(55, 55), mine=False)
    world = FakeWorld(units + army, buildings, enemies=[foe], eco=economy(population=27, cap=40))
    world.land_route = lambda x, y, tx, ty: [(x, y), (tx, ty)] if (x < 65) == (tx < 65) else None
    _, client = play(world)
    assert client.named("attack") == []


def test_far_defenders_use_short_hops_and_keep_their_pause():
    units, buildings = base(12)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(200, 55)) for i in range(14)]
    foe = unit(900, UnitKind.SPEARMAN, tile=(55, 55), mine=False)
    world = lambda tick: FakeWorld(units + army, buildings, enemies=[foe], tick=tick,
                                   eco=economy(population=27, cap=40))
    brain, client = play(world(100))
    assert len([a for a in army_moves(client) if a[0] >= 300]) == 14
    assert client.named("attack") == []
    _, client = play(world(120), brain=brain)
    assert [a for a in army_moves(client) if a[0] >= 300] == []


def test_disconnected_land_queries_are_bounded_and_retried_after_the_cooldown():
    from aoe_ai.context import Context
    from aoe_ai.config import Config
    from aoe_ai.blackboard import Blackboard
    from fakes import FakeClient, decision
    board, config, calls = Blackboard(), Config(), []
    world = FakeWorld(tick=100)
    world.land_route = lambda *args: calls.append(args)
    def context(tick):
        world.tick = tick
        return Context(FakeClient(), decision(world), board, config)
    for tick in (100, 120, 200):
        assert context(tick).land_route((10, 10), (20, 10)) is None
    assert len(calls) == 1
    assert context(301).land_route((10, 10), (20, 10)) is None
    assert len(calls) == 2
    assert len(board.land_failures) <= config.land_cache_entries


def test_crowded_defence_leaves_query_budget_to_issue_orders():
    units, buildings = base(12)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(14)]
    foes = [unit(900 + i, UnitKind.SPEARMAN, tile=(42 + i % 8, 45 + i // 8), mine=False) for i in range(32)]
    _, client = play(FakeWorld(units + army, buildings, enemies=foes,
                               eco=economy(population=27, cap=40)))
    assert [a for a in client.named("attack") if a[0] >= 300]
