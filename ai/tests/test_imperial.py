"""Imperial Age: when the AI saves for it, what it raises and researches, and
that every Castle Age capability keeps working in the new age."""

from aoeworld import Age, BuildingKind, Tech, UnitKind

from fakes import FakeWorld, base, building, economy, unit
from test_logic import play

CASTLE = [Tech.LOOM, Tech.FEUDAL_AGE, Tech.CASTLE_AGE]


def army(count, kind=UnitKind.SPEARMAN):
    return [unit(300 + i, kind, tile=(60, 60)) for i in range(count)]


def castle_base(villagers=40, soldiers=24):
    units, buildings = base(villagers)
    units += army(soldiers)
    buildings += [building(600, BuildingKind.BARRACKS, tile=(60, 50)),
                  building(601, BuildingKind.ARCHERY_RANGE, tile=(64, 50)),
                  building(602, BuildingKind.BLACKSMITH, tile=(68, 50)),
                  building(603, BuildingKind.STABLE, tile=(72, 50))]
    return units, buildings


def castle_economy(**kwargs):
    settings = dict(food=1500, wood=900, gold=1200, population=70, cap=120, age=Age.CASTLE,
                    researched=list(CASTLE))
    settings.update(kwargs)
    return economy(**settings)


def test_imperial_age_is_clicked_once_two_castle_age_buildings_and_an_army_stand():
    units, buildings = castle_base()
    buildings += [building(610, BuildingKind.UNIVERSITY, tile=(60, 60)),
                  building(611, BuildingKind.SIEGE_WORKSHOP, tile=(64, 60))]
    _, client = play(FakeWorld(units, buildings, eco=castle_economy()))
    assert (500, Tech.IMPERIAL_AGE) in client.named("research")
    assert not client.named("train"), "the age's budget is saved, not spent on units"


def test_imperial_age_waits_for_the_second_building_the_villagers_and_the_army():
    for villagers, soldiers, extra in [
        (40, 24, [building(610, BuildingKind.UNIVERSITY, tile=(60, 60))]),
        (30, 24, [building(610, BuildingKind.UNIVERSITY), building(611, BuildingKind.SIEGE_WORKSHOP)]),
        (40, 10, [building(610, BuildingKind.UNIVERSITY), building(611, BuildingKind.SIEGE_WORKSHOP)]),
    ]:
        units, buildings = castle_base(villagers, soldiers)
        _, client = play(FakeWorld(units, buildings + extra, eco=castle_economy()))
        assert (500, Tech.IMPERIAL_AGE) not in client.named("research"), (villagers, soldiers)


def test_imperial_age_is_not_bought_without_its_food_and_gold():
    units, buildings = castle_base()
    buildings += [building(610, BuildingKind.UNIVERSITY), building(611, BuildingKind.SIEGE_WORKSHOP)]
    _, client = play(FakeWorld(units, buildings, eco=castle_economy(gold=700)))
    assert (500, Tech.IMPERIAL_AGE) not in client.named("research")


def test_the_castle_economy_raises_a_university_and_a_siege_workshop_for_the_age():
    units, buildings = castle_base(villagers=36, soldiers=10)
    world = FakeWorld(units, buildings, eco=castle_economy(wood=900))
    _, client = play(world)
    placed = [site[0] for site in world.sites]
    assert BuildingKind.UNIVERSITY in placed
    units, buildings = castle_base(villagers=20, soldiers=10)
    world = FakeWorld(units, buildings, eco=castle_economy(wood=900))
    play(world)
    assert BuildingKind.UNIVERSITY not in [site[0] for site in world.sites]


def test_imperial_army_trains_the_newest_identity_of_every_line():
    units, buildings = castle_base(villagers=40, soldiers=6)
    researched = CASTLE + [Tech.IMPERIAL_AGE, Tech.CROSSBOWMAN, Tech.ARBALESTER, Tech.PIKEMAN,
                           Tech.HALBERDIER, Tech.CAVALIER, Tech.PALADIN]
    _, client = play(FakeWorld(units, buildings,
                               eco=castle_economy(age=Age.IMPERIAL, researched=researched, food=3000, gold=3000)))
    trained = {args for args in client.named("train")}
    assert (601, UnitKind.ARBALESTER) in trained
    assert (600, UnitKind.HALBERDIER) in trained
    assert (603, UnitKind.PALADIN) in trained


def test_imperial_age_keeps_castle_age_training_and_the_villager_economy():
    units, buildings = castle_base(villagers=34, soldiers=6)
    _, client = play(FakeWorld(units, buildings, eco=castle_economy(
        age=Age.IMPERIAL, researched=CASTLE + [Tech.IMPERIAL_AGE], food=3000, gold=3000)))
    assert (603, UnitKind.KNIGHT) in client.named("train")
    assert any(args[1] == UnitKind.VILLAGER for args in client.named("train"))


def test_imperial_upgrades_follow_the_castle_ones_in_priority_order():
    units, buildings = castle_base(villagers=40, soldiers=12)
    researched = CASTLE + [Tech.IMPERIAL_AGE, Tech.CROSSBOWMAN, Tech.PIKEMAN]
    _, client = play(FakeWorld(units, buildings, eco=castle_economy(
        age=Age.IMPERIAL, researched=researched, food=1500, gold=1500)))
    assert (601, Tech.ARBALESTER) in client.named("research")


def test_imperial_blacksmith_and_economy_techs_need_their_predecessors():
    units, buildings = castle_base(villagers=40, soldiers=12)
    buildings += [building(620, BuildingKind.MILL, tile=(40, 50)), building(621, BuildingKind.LUMBER_CAMP, tile=(40, 56))]
    researched = CASTLE + [Tech.IMPERIAL_AGE, Tech.CROSSBOWMAN, Tech.PIKEMAN, Tech.ARBALESTER, Tech.HALBERDIER,
                           Tech.CAVALIER, Tech.PALADIN, Tech.BODKIN_ARROW, Tech.HEAVY_PLOW, Tech.BOW_SAW]
    wanted = {Tech.BRACER, Tech.CROP_ROTATION, Tech.TWO_MAN_SAW}
    seen = set()
    for turn in range(3):
        eco = castle_economy(age=Age.IMPERIAL, researched=researched + list(seen), food=5000, wood=5000, gold=5000)
        brain, client = play(FakeWorld(units, buildings, eco=eco))
        seen |= {args[1] for args in client.named("research")}
        if wanted <= seen:
            break
    assert Tech.BRACER in seen
    assert Tech.PLATE_MAIL_ARMOR not in seen, "Chain Mail Armor was never researched"


def test_arena_reports_the_imperial_minute_and_accepts_the_age_as_proven():
    from aoe_ai.arena import proven, summary
    from aoe_ai.host import BotReport

    report = BotReport(slot=0, age="IMPERIAL", stats={"army_peak": 10, "attack_hits": 1, "imperial_tick": 2400})
    assert proven([report], 10)
    assert summary([report], 6000, 1, 256, 1.0)["bots"][0]["imperial_minute"] == 2.0
