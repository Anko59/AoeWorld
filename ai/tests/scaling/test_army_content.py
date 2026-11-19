"""The standard brain develops and spends on the live production roster."""
from aoeworld import Age, BuildingKind, Tech, UnitKind, tech_cost

from aoe_ai import Brain, Config
from aoe_ai.blackboard import Blackboard
from aoe_ai.context import Context
from aoe_ai.play.military import MilitaryManager
from aoe_ai.play.research import ResearchManager
from fakes import FakeClient, FakeWorld, base, building, decision, economy, unit


def position(*production, age=Age.IMPERIAL, researched=(), stock=(5000, 5000, 5000, 5000),
             army=(), enemies=(), enemy_buildings=()):
    workers, buildings = base(45)
    food, wood, gold, stone = stock
    eco = economy(food=food, wood=wood, gold=gold, stone=stone, age=age,
                  researched=[Tech.LOOM, Tech.FEUDAL_AGE, Tech.CASTLE_AGE, *researched],
                  population=len(workers) + len(army), cap=150)
    world = FakeWorld(workers + list(army), buildings + list(production), eco=eco,
                      enemies=enemies, enemy_buildings=enemy_buildings)
    cfg = Config(save_for_management=False, castles=0, generals=0, stewards=0)
    return Context(FakeClient(), decision(world), Blackboard(), cfg)


def test_stable_adds_camels_when_the_visible_army_is_mounted():
    ctx = position(building(600, BuildingKind.STABLE),
                   enemies=[unit(900 + i, UnitKind.KNIGHT, mine=False) for i in range(10)])
    MilitaryManager().train(ctx)
    assert (600, UnitKind.CAMEL_RIDER) in ctx.client.named("train")


def test_castle_archery_has_a_mounted_line():
    archers = [unit(900 + i, UnitKind.ARCHER) for i in range(20)]
    skirmishers = [unit(940 + i, UnitKind.SKIRMISHER) for i in range(20)]
    ctx = position(building(600, BuildingKind.ARCHERY_RANGE), age=Age.CASTLE,
                   army=archers + skirmishers)
    MilitaryManager().train(ctx)
    assert (600, UnitKind.CAVALRY_ARCHER) in ctx.client.named("train")


def test_siege_workshop_and_castle_answer_enemy_fortifications():
    target = building(999, BuildingKind.CASTLE, mine=False)
    ctx = position(building(600, BuildingKind.SIEGE_WORKSHOP), building(601, BuildingKind.CASTLE),
                   enemy_buildings=[target])
    MilitaryManager().train(ctx)
    trained = dict(ctx.client.named("train"))
    assert trained[600] == UnitKind.BATTERING_RAM
    assert trained[601] in (UnitKind.TREBUCHET, UnitKind.PETARD)


def test_chemistry_unlocks_gunpowder_choices_with_their_real_cost():
    army = [unit(900 + i, kind) for i, kind in enumerate(
        [UnitKind.ARCHER, UnitKind.SKIRMISHER, UnitKind.CAVALRY_ARCHER] * 20)]
    ctx = position(building(600, BuildingKind.ARCHERY_RANGE), researched=[Tech.CHEMISTRY],
                   army=army)
    MilitaryManager().train(ctx)
    assert (600, UnitKind.HAND_CANNONEER) in ctx.client.named("train")


def test_affordable_unit_upgrade_drains_one_queue_and_leaves_another_to_train():
    tech = Tech.ELITE_SKIRMISHER
    first = building(600, BuildingKind.ARCHERY_RANGE, queue=1)
    second = building(601, BuildingKind.ARCHERY_RANGE, queue=1)
    army = [unit(900 + i, UnitKind.SKIRMISHER) for i in range(14)]
    ctx = position(first, second, army=army)
    ResearchManager().step(ctx)
    MilitaryManager().train(ctx)
    assert all(b != 600 for b, _ in ctx.client.named("train"))
    assert any(b == 601 for b, _ in ctx.client.named("train"))
    ready = position(building(600, BuildingKind.ARCHERY_RANGE), army=army,
                     stock=tech_cost(tech))
    ResearchManager().step(ready)
    assert (600, tech) in ready.client.named("research")


def test_unaffordable_line_does_not_block_an_affordable_upgrade():
    tech = Tech.ELITE_SKIRMISHER
    ctx = position(building(600, BuildingKind.STABLE), building(601, BuildingKind.ARCHERY_RANGE),
                   army=[unit(900 + i, UnitKind.KNIGHT) for i in range(50)],
                   stock=tech_cost(tech))
    ResearchManager().step(ctx)
    assert (601, tech) in ctx.client.named("research")
    assert not ctx.saving_castle


def test_research_and_training_do_not_claim_the_same_idle_building():
    ctx = position(building(600, BuildingKind.STABLE), army=[unit(900, UnitKind.KNIGHT)])
    Brain(ctx.config).decide(ctx.client, decision(ctx.world))
    assert any(b == 600 for b, _ in ctx.client.named("research"))
    assert all(b != 600 for b, _ in ctx.client.named("train"))


def test_existing_dock_defends_against_new_warships_after_fish_are_gone():
    from aoe_ai.play.constructor import Constructor
    from aoe_ai.play.naval import NavalManager

    first = building(600, BuildingKind.DOCK, tile=(55, 50))
    enemy = unit(900, UnitKind.CARRACK, tile=(61, 55), mine=False)
    ctx = position(first, enemies=[enemy],
                   researched=[Tech.WAR_GALLEY, Tech.HEAVY_WARSHIPS])
    NavalManager(Constructor()).step(ctx)
    [(dock, kind)] = ctx.client.named("train")
    assert dock == first.id
    assert kind in (UnitKind.GALLEON, UnitKind.FAST_FIRE_SHIP, UnitKind.CARRACK,
                    UnitKind.CANNON_GALLEON, UnitKind.CATAPULT_GALLEON,
                    UnitKind.DEMOLITION_RAFT)


def test_arms_research_includes_chemistry_husbandry_and_ready_blacksmith_techs():
    ready = [Tech.CHEMISTRY, Tech.HUSBANDRY, Tech.FORGING]
    base_kinds = [BuildingKind.UNIVERSITY, BuildingKind.STABLE, BuildingKind.BLACKSMITH]
    choices = [building(610 + i, kind) for i, kind in enumerate(base_kinds)]
    choices += [building(630, BuildingKind.SIEGE_WORKSHOP)]
    for wanted, source in zip(ready, choices):
        from aoeworld import technologies

        completed = [r.tech for r in technologies() if r.tech != wanted]
        ctx = position(*choices, researched=completed, stock=tech_cost(wanted))
        ResearchManager().step(ctx)
        assert (source.id, wanted) in ctx.client.named("research")


def test_imperial_research_leaves_sibling_producers_working_until_the_upgrade_finishes():
    first = building(600, BuildingKind.STABLE, researching=Tech.CAVALIER)
    sibling = building(601, BuildingKind.STABLE)
    ctx = position(first, sibling, army=[unit(900, UnitKind.KNIGHT)])
    ResearchManager().step(ctx)
    MilitaryManager().train(ctx)
    assert all(b not in (first.id, sibling.id) for b, _ in ctx.client.named("research"))
    assert [b for b, _ in ctx.client.named("train")] == [sibling.id]


def test_an_established_imperial_line_can_buy_its_upgrade_before_optional_management():
    tech = Tech.HALBERDIER
    ctx = position(building(600, BuildingKind.BARRACKS), building(601, BuildingKind.CASTLE),
                   researched=[Tech.PIKEMAN], stock=tech_cost(tech),
                   army=[unit(900 + i, UnitKind.PIKEMAN) for i in range(16)])
    ctx.config.save_for_management = True
    ctx.config.generals = ctx.config.stewards = 1
    ResearchManager().step(ctx)
    assert (600, tech) in ctx.client.named("research")


def test_a_castle_age_mounted_army_can_research_husbandry():
    tech = Tech.HUSBANDRY
    ctx = position(building(600, BuildingKind.STABLE), age=Age.CASTLE,
                   researched=[Tech.BLOODLINES, Tech.LIGHT_CAVALRY], stock=tech_cost(tech),
                   army=[unit(900 + i, UnitKind.CAMEL_RIDER) for i in range(6)])
    ctx.board.stats["castle_units_peak"] = len(ctx.army)
    ResearchManager().step(ctx)
    assert (600, tech) in ctx.client.named("research")
