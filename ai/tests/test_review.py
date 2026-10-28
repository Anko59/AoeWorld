"""Regression tests for review findings: hit proof, refused approach tiles,
the per-decision job cap and abandoned foundations."""

from aoeworld import Age, BuildingKind, CommandResult, Tech, UnitKind, UnitTask

from aoe_ai import Brain
from aoe_ai.play.economy import MAX_ASSIGN_PER_DECISION

from fakes import FakeClient, FakeWorld, base, building, decision, economy, order_result, unit
from test_logic import play


def armed(count=12, tile=(60, 60)):
    return [unit(300 + i, UnitKind.SPEARMAN, tile=tile) for i in range(count)]


def test_an_unexplored_build_refusal_releases_the_plan_without_blacklisting_the_site():
    units, buildings = base(8)
    world = FakeWorld(units, buildings, eco=economy(wood=100, population=11, cap=15))
    brain, client = play(world)
    sequence, args = next((i + 1, a) for i, (name, a) in enumerate(client.calls) if name == "build")
    from aoe_ai.context import Context
    tracked = brain.board.tracked[sequence]
    brain.on_rejected(Context(FakeClient(), decision(world), brain.board, brain.config),
                      tracked, CommandResult.REJECTED_UNEXPLORED_PLACEMENT)
    assert not brain.board.plans
    assert not brain.board.builders
    assert (str(args[1]), args[2], args[3]) not in brain.board.bad_sites


def test_a_foundation_rising_in_health_is_not_a_landed_blow():
    units, buildings = base(10)
    foundation = building(901, BuildingKind.HOUSE, tile=(64, 64), complete=False, mine=False, hp=100)
    brain, _ = play(FakeWorld(units + armed(), buildings, enemy_buildings=[foundation]))
    foundation.hp = 400
    play(FakeWorld(units + armed(), buildings, enemy_buildings=[foundation]), brain=brain)
    assert brain.board.attacking
    assert brain.board.stats.get("attack_hits", 0) == 0


def test_a_blow_counts_only_when_health_falls_with_our_soldiers_near():
    units, buildings = base(10)
    target = building(901, BuildingKind.BARRACKS, tile=(64, 64), mine=False, hp=1000)
    brain, _ = play(FakeWorld(units + armed(), buildings, enemy_buildings=[target]))
    target.hp = 900
    play(FakeWorld(units + armed(), buildings, enemy_buildings=[target]), brain=brain)
    assert brain.board.stats["attack_hits"] == 1
    far = building(902, BuildingKind.BARRACKS, tile=(250, 250), mine=False, hp=1000)
    brain, _ = play(FakeWorld(units + armed(), buildings, enemy_buildings=[far]))
    far.hp = 900
    play(FakeWorld(units + armed(), buildings, enemy_buildings=[far]), brain=brain)
    assert brain.board.stats.get("attack_hits", 0) == 0


def test_a_refused_approach_tile_is_not_sent_again():
    units, buildings = base(10)
    target = building(901, BuildingKind.BARRACKS, tile=(150, 120), mine=False, visible=False)
    world = lambda: FakeWorld(units + armed(), buildings, enemy_buildings=[target])  # noqa: E731
    brain, client = play(world())
    first, = client.named("formation_move")
    assert first[0] == list(range(300, 312))
    sequence = next(i + 1 for i, (n, _) in enumerate(client.calls) if n == "formation_move")
    refusal = order_result(sequence, CommandResult.REJECTED_INVALID_DESTINATION)
    _, client = play(world(), brain=brain, events=[refusal])
    second, = client.named("formation_move")
    assert second[0] == first[0]
    assert (first[1], first[2]) != (second[1], second[2])


def test_villagers_over_the_decision_cap_wait_unstamped_and_dead_ones_are_forgotten():
    units, buildings = base(MAX_ASSIGN_PER_DECISION + 4)
    brain = Brain()
    brain.board.assigned[9999] = 1  # a villager that died long ago
    client = FakeClient()
    from fakes import node
    from aoeworld import ResourceKind
    nodes = [node(i, ResourceKind.WOOD, (55 + i % 5, 55)) for i in range(30)]
    world = FakeWorld(units, buildings, nodes=nodes, eco=economy(population=20, cap=40))
    brain.decide(client, decision(world))
    stamped = {i for i in brain.board.assigned if i in {u.id for u in units}}
    assert 9999 not in brain.board.assigned
    assert len(stamped) == MAX_ASSIGN_PER_DECISION


def test_a_stalled_foundation_gets_a_villager_sent_to_construct_it():
    units, buildings = base(10)
    for worker in units:
        worker.idle, worker.task = False, UnitTask.GATHERING
    foundation = building(700, BuildingKind.BARRACKS, tile=(56, 56), complete=False)
    foundation.progress = 50
    brain, _ = play(FakeWorld(units, buildings + [foundation], tick=100))
    later = FakeWorld(units, buildings + [foundation], tick=100 + 20 * 9)
    _, client = play(later, brain=brain)
    assert [a for a in client.named("construct") if a[1] == 700]


def test_castle_age_saves_resources_drains_tc_and_uses_two_feudal_kinds():
    units, buildings = base(34)
    units += armed()
    buildings += [building(600, BuildingKind.ARCHERY_RANGE, tile=(60, 50)),
                  building(601, BuildingKind.BLACKSMITH, tile=(64, 50))]
    eco = economy(food=850, wood=500, gold=250, population=35, cap=60,
                  age=Age.FEUDAL, researched=[Tech.LOOM, Tech.FEUDAL_AGE])
    _, client = play(FakeWorld(units, buildings, eco=eco))
    assert (500, Tech.CASTLE_AGE) in client.named("research")
    assert not client.named("train"), "save the Castle budget while researching"


def test_castle_army_trains_knights_and_keeps_feudal_economy_active():
    units, buildings = base(34)
    buildings += [building(600, BuildingKind.STABLE, tile=(60, 50)),
                  building(601, BuildingKind.ARCHERY_RANGE, tile=(64, 50))]
    eco = economy(food=900, wood=900, gold=900, population=35, cap=60,
                  age=Age.CASTLE, researched=[Tech.LOOM, Tech.FEUDAL_AGE, Tech.CASTLE_AGE])
    _, client = play(FakeWorld(units, buildings, eco=eco))
    assert (600, UnitKind.KNIGHT) in client.named("train")
    assert any(args[1] == UnitKind.VILLAGER for args in client.named("train"))


def test_arena_feudal_milestone_also_accepts_castle_without_accepting_dark_age():
    from aoe_ai.arena import proven
    from aoe_ai.host import BotReport

    report = BotReport(slot=0, age="CASTLE", stats={"army_peak": 10, "attack_hits": 1})
    assert proven([report], 10)
    report.age = "DARK"
    assert not proven([report], 10)


def test_castle_saving_waits_until_a_feudal_army_is_established():
    units, buildings = base(34)
    units += armed(4)
    buildings += [building(600, BuildingKind.ARCHERY_RANGE, tile=(60, 50)),
                  building(601, BuildingKind.BLACKSMITH, tile=(64, 50))]
    _, client = play(FakeWorld(units, buildings, eco=economy(food=900, wood=900, gold=900,
        population=39, cap=60, age=Age.FEUDAL, researched=[Tech.LOOM, Tech.FEUDAL_AGE])))
    assert (500, Tech.CASTLE_AGE) not in client.named("research")
    assert any(args[1] != UnitKind.VILLAGER for args in client.named("train"))
    assert not any(args[1] == UnitKind.VILLAGER for args in client.named("train"))


def test_castle_prerequisite_wood_is_not_spent_on_an_endless_feudal_army():
    units, buildings = base(34)
    buildings += [building(600, BuildingKind.ARCHERY_RANGE, tile=(60, 50))]
    _, client = play(FakeWorld(units, buildings, eco=economy(food=900, wood=100, gold=900,
        population=35, cap=60, age=Age.FEUDAL, researched=[Tech.LOOM, Tech.FEUDAL_AGE])))
    assert not [args for args in client.named("train") if args[1] != UnitKind.VILLAGER]


def test_castle_prerequisite_wood_accumulates_before_new_farms():
    units, buildings = base(34)
    buildings += [building(600, BuildingKind.MILL), building(601, BuildingKind.LUMBER_CAMP),
                  building(602, BuildingKind.BARRACKS), building(603, BuildingKind.ARCHERY_RANGE)]
    _, client = play(FakeWorld(units, buildings, eco=economy(food=900, wood=100, gold=900,
        population=35, cap=100, age=Age.FEUDAL, researched=[Tech.LOOM, Tech.FEUDAL_AGE])))
    assert not [a for a in client.named("build") if a[1] == BuildingKind.FARM]


def test_castle_blacksmith_is_placed_before_farms_can_consume_its_budget():
    units, buildings = base(34)
    buildings += [building(600, BuildingKind.MILL), building(601, BuildingKind.LUMBER_CAMP),
                  building(602, BuildingKind.BARRACKS), building(603, BuildingKind.ARCHERY_RANGE)]
    _, client = play(FakeWorld(units, buildings, eco=economy(food=900, wood=200, gold=900,
        population=35, cap=100, age=Age.FEUDAL,
        researched=[Tech.LOOM, Tech.FEUDAL_AGE, Tech.DOUBLE_BIT_AXE, Tech.WHEELBARROW])))
    kinds = [a[1] for a in client.named("build")]
    assert BuildingKind.BLACKSMITH in kinds
    assert BuildingKind.FARM not in kinds


def test_castle_wood_is_also_reserved_when_the_range_is_the_missing_prerequisite():
    units, buildings = base(34)
    buildings += [building(600, BuildingKind.MILL), building(601, BuildingKind.LUMBER_CAMP),
                  building(602, BuildingKind.BARRACKS), building(603, BuildingKind.BLACKSMITH)]
    _, client = play(FakeWorld(units, buildings, eco=economy(food=900, wood=100, gold=900,
        population=35, cap=100, age=Age.FEUDAL, researched=[Tech.LOOM, Tech.FEUDAL_AGE])))
    assert not [a for a in client.named("build") if a[1] == BuildingKind.FARM]
    assert not [a for a in client.named("train") if a[1] != UnitKind.VILLAGER]


def test_castle_prerequisite_saving_starts_when_feudal_begins_before_thirty_two_workers():
    for villagers, wood in [(22, 100), (22, 200), (31, 100)]:
        units, buildings = base(villagers)
        buildings += [building(600, BuildingKind.MILL), building(601, BuildingKind.LUMBER_CAMP),
                      building(602, BuildingKind.BARRACKS), building(603, BuildingKind.ARCHERY_RANGE)]
        _, client = play(FakeWorld(units, buildings, eco=economy(food=900, wood=wood, gold=900,
            population=villagers + 1, cap=100, age=Age.FEUDAL,
            researched=[Tech.LOOM, Tech.FEUDAL_AGE, Tech.DOUBLE_BIT_AXE, Tech.WHEELBARROW])))
        kinds = [a[1] for a in client.named("build")]
        assert BuildingKind.FARM not in kinds
        if wood == 200:
            assert BuildingKind.BLACKSMITH in kinds


def test_castle_saving_redirects_empty_gold_workers_to_food_without_losing_loaded_returns():
    from aoeworld import ResourceKind
    from fakes import node

    for soldiers in (4, 12):
        units, buildings = base(34)
        units += armed(soldiers)
        empty = unit(700, tile=(52, 50), task=UnitTask.GATHERING, gathering=ResourceKind.GOLD)
        loaded = unit(701, tile=(54, 50), task=UnitTask.RETURNING, gathering=ResourceKind.GOLD)
        loaded.carrying = ResourceKind.GOLD
        units += [empty, loaded]
        buildings += [building(600, BuildingKind.ARCHERY_RANGE), building(601, BuildingKind.BLACKSMITH)]
        food = node(900, ResourceKind.FOOD, (48, 52))
        _, client = play(FakeWorld(units, buildings, nodes=[food], eco=economy(food=100, wood=300,
            gold=900, population=49, cap=100, age=Age.FEUDAL,
            researched=[Tech.LOOM, Tech.FEUDAL_AGE])))
        assert (empty.id, food.id) in client.named("gather")
        assert not [a for a in client.named("gather") if a[0] == loaded.id]
        if soldiers == 12:
            assert not client.named("train")


def test_castle_preparation_defers_optional_upgrades_while_the_feudal_army_forms():
    units, buildings = base(34)
    units += armed(8)
    buildings += [building(600, BuildingKind.BARRACKS), building(601, BuildingKind.LUMBER_CAMP),
                  building(602, BuildingKind.ARCHERY_RANGE), building(603, BuildingKind.BLACKSMITH)]
    _, client = play(FakeWorld(units, buildings, eco=economy(food=300, wood=300, gold=900,
        population=43, cap=100, age=Age.FEUDAL, researched=[Tech.LOOM, Tech.FEUDAL_AGE])))
    assert not client.named("research")
    assert not [a for a in client.named("train") if a[1] == UnitKind.VILLAGER]
    assert [a for a in client.named("train") if a[1] != UnitKind.VILLAGER]


def test_castle_army_minimum_is_funded_before_more_farms_consume_its_wood():
    units, buildings = base(34)
    units += armed(4)
    buildings += [building(600, BuildingKind.BARRACKS), building(601, BuildingKind.LUMBER_CAMP),
                  building(602, BuildingKind.ARCHERY_RANGE), building(603, BuildingKind.BLACKSMITH),
                  building(604, BuildingKind.MILL)]
    _, client = play(FakeWorld(units, buildings, eco=economy(food=700, wood=135, gold=900,
        population=39, cap=100, age=Age.FEUDAL, researched=[Tech.LOOM, Tech.FEUDAL_AGE])))
    assert {a[0] for a in client.named("train")} == {600, 602}
    first_farm = next((i for i, (name, args) in enumerate(client.calls)
                       if name == "build" and args[1] == BuildingKind.FARM), len(client.calls))
    assert all(i < first_farm for i, (name, _) in enumerate(client.calls) if name == "train")


def test_castle_unit_upgrade_precedes_optional_feudal_economy_upgrades():
    units, buildings = base(34)
    units += armed()
    buildings += [building(600, BuildingKind.ARCHERY_RANGE), building(601, BuildingKind.BLACKSMITH),
                  building(602, BuildingKind.LUMBER_CAMP)]
    _, client = play(FakeWorld(units, buildings, eco=economy(food=900, wood=900, gold=900,
        population=47, cap=100, age=Age.CASTLE,
        researched=[Tech.LOOM, Tech.FEUDAL_AGE, Tech.CASTLE_AGE])))
    assert (600, Tech.CROSSBOWMAN) in client.named("research")


def test_castle_preparation_protects_food_at_twenty_eight_workers_without_skipping_the_army():
    units, buildings = base(28)
    units += armed(4)
    buildings += [building(600, BuildingKind.BARRACKS), building(601, BuildingKind.ARCHERY_RANGE),
                  building(602, BuildingKind.BLACKSMITH)]
    _, client = play(FakeWorld(units, buildings, eco=economy(food=900, wood=900, gold=900,
        population=33, cap=60, age=Age.FEUDAL, researched=[Tech.LOOM, Tech.FEUDAL_AGE])))
    assert not [a for a in client.named("train") if a[1] == UnitKind.VILLAGER]
    assert [a for a in client.named("train") if a[1] != UnitKind.VILLAGER]
    assert (500, Tech.CASTLE_AGE) not in client.named("research")


def test_castle_preparation_keeps_gold_income_for_the_first_castle_unit():
    from aoe_ai.context import Context
    units, buildings = base(30)
    world = FakeWorld(units, buildings, eco=economy(food=400, gold=220,
        age=Age.FEUDAL, researched=[Tech.LOOM, Tech.FEUDAL_AGE]))
    brain = Brain()
    ctx = Context(FakeClient(), decision(world), brain.board, brain.config)
    assert brain.economy.shares(ctx)["GOLD"] > 0


def test_first_castle_army_upgrade_uses_its_exact_budget_before_optional_spending():
    units, buildings = base(30)
    units.append(unit(300, UnitKind.ARCHER))
    buildings.append(building(600, BuildingKind.ARCHERY_RANGE))
    world = FakeWorld(units, buildings, eco=economy(food=175, wood=0, gold=100,
        population=32, cap=60, age=Age.CASTLE,
        researched=[Tech.LOOM, Tech.FEUDAL_AGE, Tech.CASTLE_AGE]))
    _, client = play(world)
    assert (600, Tech.CROSSBOWMAN) in client.named("research")


def test_first_castle_army_upgrade_stops_new_training_that_would_spend_its_food():
    units, buildings = base(30)
    units += [unit(300, UnitKind.ARCHER), unit(301, UnitKind.SPEARMAN)]
    buildings += [building(600, BuildingKind.ARCHERY_RANGE, queue=1), building(601, BuildingKind.BARRACKS)]
    world = FakeWorld(units, buildings, eco=economy(food=110, wood=500, gold=500,
        population=33, cap=60, age=Age.CASTLE,
        researched=[Tech.LOOM, Tech.FEUDAL_AGE, Tech.CASTLE_AGE]))
    _, client = play(world)
    assert not client.named("train")
    assert not client.named("research")


def test_first_castle_army_pause_ends_once_its_upgrade_is_researching_or_bought():
    # Training resumes while the first upgrade researches and after either is bought.
    for range_tech, researched in ((Tech.CROSSBOWMAN, []), (None, [Tech.CROSSBOWMAN])):
        units, buildings = base(30)
        units += [unit(300, UnitKind.ARCHER), unit(301, UnitKind.SPEARMAN)]
        buildings += [building(600, BuildingKind.ARCHERY_RANGE, researching=range_tech),
                      building(601, BuildingKind.BARRACKS)]
        world = FakeWorld(units, buildings, eco=economy(food=400, wood=500, gold=500,
            population=33, cap=60, age=Age.CASTLE,
            researched=[Tech.LOOM, Tech.FEUDAL_AGE, Tech.CASTLE_AGE, *researched]))
        _, client = play(world)
        assert client.named("train"), (range_tech, researched)
