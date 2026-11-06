"""Management units: Stewards, Vassals and Generals are trained, seated and led."""

from dataclasses import replace

from aoeworld import BuildingKind, GeneralState, UnitKind

from aoe_ai.config import Config
from fakes import FakeWorld, base, building, economy, unit
from test_imperial import castle_base, castle_economy
from test_logic import play


def rich():
    return castle_economy(food=3000, wood=900, gold=3000, stone=900, population=70, cap=150)


def test_a_steward_is_trained_at_the_town_center_and_then_seated():
    units, buildings = castle_base()
    config = replace(Config(), stewards=1, steward_min_villagers=20)
    _, client = play(FakeWorld(units, buildings, eco=rich()), config)
    assert (500, UnitKind.STEWARD) in client.named("train")
    steward = unit(900, UnitKind.STEWARD, tile=(52, 52))
    _, client = play(FakeWorld(units + [steward], buildings, eco=rich()), config)
    assert (900, 500) in client.named("garrison")


def test_a_seated_steward_is_not_sent_again_and_a_second_is_not_trained():
    units, buildings = castle_base()
    buildings[0].garrison = [(900, UnitKind.STEWARD, 50, 50)]
    config = replace(Config(), stewards=1, steward_min_villagers=20)
    _, client = play(FakeWorld(units, buildings, eco=rich()), config)
    assert (500, UnitKind.STEWARD) not in client.named("train")
    assert not client.named("garrison")


def test_a_castle_is_raised_with_stone_and_hosts_a_vassal_then_a_general():
    units, buildings = castle_base()
    config = replace(Config(), castles=1, vassals=1, generals=1)
    _, client = play(FakeWorld(units, buildings, eco=rich()), config)
    assert any(args[1] == BuildingKind.CASTLE for args in client.named("build"))
    buildings.append(building(700, BuildingKind.CASTLE, tile=(80, 60)))
    _, client = play(FakeWorld(units, buildings, eco=rich()), config)
    assert (700, UnitKind.VASSAL) in client.named("train")
    buildings[-1].garrison = [(901, UnitKind.VASSAL, 80, 60)]
    _, client = play(FakeWorld(units, buildings, eco=rich()), config)
    assert (700, UnitKind.GENERAL) in client.named("train")


def test_a_new_general_gets_train_and_mix_and_marches_a_big_army_at_the_enemy():
    units, buildings = castle_base()
    general = unit(902, UnitKind.GENERAL, tile=(60, 60))
    general.army_size, general.general_state = 40, GeneralState.IDLE
    enemy = building(800, BuildingKind.HOUSE, tile=(200, 200), mine=False)
    config = replace(Config(), generals=1, general_attack_army=30)
    brain, client = play(FakeWorld(units + [general], buildings, eco=rich(), enemy_buildings=[enemy]), config)
    assert (902, True) in client.named("general_train")
    assert client.named("general_mix")[0][0] == 902
    assert any(args[0] == 902 for args in client.named("move"))


def test_soldiers_a_general_leads_get_no_orders_from_the_ai():
    units, buildings = castle_base(soldiers=0)
    led = [unit(950 + i, UnitKind.SPEARMAN, tile=(70, 70)) for i in range(12)]
    for soldier in led:
        soldier.commanded_by = 902
    _, client = play(FakeWorld(units + led, buildings, eco=rich(), enemy_buildings=[]), Config())
    moved = {args[0] for args in client.named("move")} | {u for args in client.named("formation_move") for u in args[0]}
    assert not moved & {s.id for s in led}


def test_saving_for_a_steward_stops_soldiers_but_gives_up_after_the_limit():
    units, buildings = castle_base(soldiers=3)
    config = replace(Config(), stewards=1, steward_min_villagers=20, save_seconds=60)
    short = castle_economy(food=900, gold=900, wood=900, population=50, cap=150)
    brain, client = play(FakeWorld(units, buildings, eco=short), config)
    assert not [a for a in client.named("train") if a[1] != UnitKind.VILLAGER], "soldiers wait while saving"
    late = FakeWorld(units, buildings, eco=short, tick=100 + 70 * 20)
    from fakes import FakeClient, decision

    client = FakeClient()
    brain.decide(client, decision(late))
    assert [a for a in client.named("train") if a[1] != UnitKind.VILLAGER], "the army spends again after the limit"
