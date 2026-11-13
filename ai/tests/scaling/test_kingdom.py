"""Kingdom growth must follow living settlements rather than fixed ceilings."""
from types import SimpleNamespace

from aoeworld import Age, BuildingKind, ResourceKind, Tech, TradeResource, UnitKind

from aoe_ai.blackboard import Blackboard
from aoe_ai.config import Config
from aoe_ai.context import Context
from aoe_ai.play import buildorder
from aoe_ai.play.constructor import Constructor
from aoe_ai.play.economy import EconomyManager
from aoe_ai.play.management import ManagementManager
from fakes import FakeClient, FakeWorld, base, building, decision, economy
from test_imperial import CASTLE, castle_base


def kingdom(villagers=80, centers=3, **changes):
    units, buildings = castle_base(villagers=villagers, soldiers=0)
    buildings += [building(500 + i, BuildingKind.TOWN_CENTER, tile=(50 + 70 * i, 50))
                  for i in range(1, centers)]
    settings = dict(food=10000, wood=10000, gold=10000, stone=10000,
                    population=villagers + 1, cap=2000, age=Age.CASTLE,
                    researched=CASTLE)
    settings.update(changes)
    world = FakeWorld(units, buildings, eco=economy(**settings), size=(1000, 1000))
    client = FakeClient()
    return Context(client, decision(world), Blackboard(), Config())


def test_villager_growth_continues_past_sixty_with_more_town_centers():
    ctx = kingdom(villagers=90, centers=3)
    EconomyManager(Constructor()).train_villagers(ctx)
    assert any(args[1] == UnitKind.VILLAGER for args in ctx.client.named('train'))


def test_the_next_settlement_is_wanted_even_after_twelve_town_centers():
    ctx = kingdom(villagers=600, centers=12)
    ManagementManager().expand(ctx)
    assert any(args[1] == BuildingKind.TOWN_CENTER for args in ctx.client.named('build'))


def test_a_settlement_is_placed_outside_existing_town_center_neighborhoods():
    ctx = kingdom(villagers=180, centers=4)
    ManagementManager().expand(ctx)
    centers = [b.center for b in ctx.buildings if b.kind == BuildingKind.TOWN_CENTER]
    orders = [a for a in ctx.client.named('build') if a[1] == BuildingKind.TOWN_CENTER]
    assert orders
    _, _, x, y = orders[0]
    assert min(max(abs(x - cx), abs(y - cy)) for cx, cy in centers) >= 24


def test_the_last_town_center_is_rebuilt_with_a_small_surviving_workforce():
    ctx = kingdom(villagers=3, centers=1)
    ctx.buildings = [b for b in ctx.buildings if b.kind != BuildingKind.TOWN_CENTER]
    ctx.center = None
    ManagementManager().step(ctx)
    assert any(a[1] == BuildingKind.TOWN_CENTER for a in ctx.client.named('build'))


def test_default_management_uses_a_steward_and_builds_a_castle():
    ctx = kingdom(villagers=40, centers=1)
    ManagementManager().step(ctx)
    assert any(a[1] == UnitKind.STEWARD for a in ctx.client.named('train'))
    assert any(a[1] == BuildingKind.CASTLE for a in ctx.client.named('build'))


def test_production_counts_follow_settlements():
    ctx = kingdom(villagers=240, centers=6)
    wanted = dict(buildorder.wanted_buildings(ctx))
    assert wanted[BuildingKind.BARRACKS] >= 6
    assert wanted[BuildingKind.ARCHERY_RANGE] >= 6


def test_a_large_economy_has_enough_farm_capacity_for_its_food_share():
    ctx = kingdom(villagers=160, centers=4)
    manager = EconomyManager(Constructor())
    ctx.buildings.append(building(900, BuildingKind.MILL, tile=(50, 60)))
    for i in range(35):
        ctx.buildings.append(building(1000 + i, BuildingKind.FARM, tile=(50 + i * 3, 70)))
        ctx.board.farmers[1000 + i] = 2000 + i
    worker = ctx.villagers[-1]
    worker.tile = (52, 65)
    assert manager.send_to_farm(ctx, worker)
    assert any(a[1] == BuildingKind.FARM for a in ctx.client.named('build'))


def test_far_settlement_resources_can_be_used_beyond_first_town_center_radius():
    from fakes import node, unit
    ctx = kingdom(villagers=90, centers=4)
    worker = unit(900, tile=(260, 54))
    tree = node(901, ResourceKind.WOOD, tile=(267, 54))
    ctx.world._nodes = [tree]
    assert EconomyManager(Constructor()).best_node(ctx, worker, ResourceKind.WOOD) is tree


def test_a_seated_steward_gets_its_training_switch_turned_on_without_order_spam():
    ctx = kingdom(villagers=40, centers=1)
    ctx.buildings[0].garrison = [(900, UnitKind.STEWARD, 50, 50)]
    manager = ManagementManager()
    manager.step(ctx)
    assert ctx.client.named('steward_settings')
    assert ctx.client.named('steward_settings')[0][-1] is True
    ctx.tick += 20
    before = len(ctx.client.named('steward_settings'))
    manager.step(ctx)
    assert len(ctx.client.named('steward_settings')) == before


def test_management_training_pauses_while_the_town_center_drains_for_research():
    ctx = kingdom(villagers=40, centers=1)
    ctx.buildings[0].garrison = [(900, UnitKind.STEWARD, 50, 50)]
    ctx.hold_town_center = True
    ManagementManager().step(ctx)
    assert any(a[-1] is False for a in ctx.client.named('steward_settings'))


def test_surplus_wood_is_sold_for_missing_gold_with_a_settlement_cooldown():
    from aoe_ai.play.market import MarketManager
    ctx = kingdom(villagers=30, centers=1, food=6000, wood=15000, gold=115)
    ctx.buildings.append(building(950, BuildingKind.MARKET))
    ctx.economy.food_price = ctx.economy.wood_price = ctx.economy.stone_price = 100
    manager = MarketManager()
    manager.step(ctx)
    assert ctx.client.named('market_sell') == [(950, TradeResource.WOOD)]
    assert ctx.spent[1] == 100 and ctx.spent[2] == 0, 'sale proceeds wait for the server'
    manager.step(ctx)
    assert len(ctx.client.named('market_sell')) == 1


def test_a_gold_rich_kingdom_buys_wood_when_building_is_starved():
    from aoe_ai.play.market import MarketManager
    ctx = kingdom(villagers=60, centers=1, food=1200, wood=7, gold=4000)
    ctx.buildings.append(building(950, BuildingKind.MARKET))
    ctx.economy.food_price = ctx.economy.wood_price = ctx.economy.stone_price = 100
    MarketManager().step(ctx)
    assert ctx.client.named('market_buy') == [(950, TradeResource.WOOD)]


def test_research_eventually_covers_techs_outside_the_old_army_shortlist():
    from aoe_ai.play.research import ResearchManager
    technologies = [getattr(Tech, n) for n in dir(Tech) if n[0].isupper()]
    ctx = kingdom(villagers=60, centers=1, age=Age.IMPERIAL,
                  researched=[t for t in technologies if t != Tech.TOWN_PATROL])
    ResearchManager().step(ctx)
    assert (500, Tech.TOWN_PATROL) in ctx.client.named('research')


def test_large_armies_keep_commands_bounded_and_leave_the_economy_a_turn():
    from aoe_ai.brain import Brain
    from fakes import unit
    ctx = kingdom(villagers=60, centers=2,
                  researched=CASTLE + [Tech.CROSSBOWMAN, Tech.PIKEMAN])
    ctx.world._units += [unit(3000 + i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(1000)]
    ctx.world._enemies = [unit(9000, UnitKind.MILITIA, tile=(65, 65), mine=False)]
    client = FakeClient()
    Brain().decide(client, decision(ctx.world))
    assert len(client.calls) <= 40, 'two decisions/s must stay below the 100 orders/s server budget'
    assert any(name in ('train', 'gather', 'build') for name, _ in client.calls), 'combat cannot starve development'


def test_a_vassal_is_enabled_and_given_workers_for_ordinary_fortification():
    ctx = kingdom(villagers=60, centers=1)
    ctx.world._enemy_buildings = [building(999, BuildingKind.BARRACKS, tile=(130, 80), mine=False)]
    ctx.buildings.append(building(950, BuildingKind.CASTLE, tile=(65, 65)))
    ctx.buildings[-1].garrison = [(951, UnitKind.VASSAL, 50, 50)]
    ManagementManager().step(ctx)
    assert (951, True) in ctx.client.named('vassal_development')
    assert ctx.board.vassal_workers[950]
    reserved = ctx.board.vassal_workers[950]
    assert all(a[0] not in reserved for a in ctx.client.named('gather'))


def test_order_budget_refuses_before_sending_or_reserving_more_stock():
    from aoe_ai.context import OrderBudgetExhausted
    import pytest
    ctx = kingdom()
    ctx.order_limit = 1
    ctx.order('stop', 1)
    before = list(ctx.spent)
    with pytest.raises(OrderBudgetExhausted):
        ctx.spend((50, 0, 0, 0))
    with pytest.raises(OrderBudgetExhausted):
        ctx.order('stop', 2)
    assert len(ctx.client.calls) == 1
    assert ctx.spent == before


def test_production_is_built_beside_a_settlement_that_needs_it():
    ctx = kingdom(villagers=180, centers=4)
    Constructor().production(ctx)
    placed = [a for a in ctx.client.named('build') if a[1] == BuildingKind.STABLE]
    assert placed and placed[0][2] > 100, 'the original core already has its Stable'


def test_optional_imperial_research_can_save_gold_ahead_of_continuous_army_training():
    from aoe_ai.play.research import ResearchManager
    ctx = kingdom(villagers=60, centers=1, age=Age.IMPERIAL, food=5000, gold=0,
                  researched=CASTLE + [Tech.IMPERIAL_AGE, Tech.CROSSBOWMAN, Tech.PIKEMAN])
    ctx.buildings[0].garrison = [(990, UnitKind.STEWARD, 50, 50)]
    ResearchManager().step(ctx)
    assert ctx.saving_castle and ctx.draining
    assert not ctx.client.named('research')


def test_town_center_queue_does_not_drain_for_an_unfunded_optional_upgrade():
    from aoe_ai.play.research import ResearchManager
    ctx = kingdom(villagers=34, centers=1, food=200, wood=60, gold=900,
                  researched=CASTLE + [Tech.TOWN_WATCH, Tech.DOUBLE_BIT_AXE, Tech.HORSE_COLLAR])
    ctx.buildings[0].queue_length = 2
    ctx.buildings[0].idle = False
    ResearchManager().step(ctx)
    assert not ctx.hold_town_center, 'the padded research budget is not available yet'


def test_imperial_prerequisite_is_funded_before_more_farms_and_dropoffs():
    from aoe_ai.brain import Brain
    from fakes import unit
    ctx = kingdom(villagers=40, centers=2, food=2000, wood=250, gold=2000,
                  researched=CASTLE + [Tech.CROSSBOWMAN, Tech.PIKEMAN])
    ctx.world._buildings.append(building(950, BuildingKind.CASTLE, tile=(80, 80)))
    ctx.world._units += [unit(3000 + i, UnitKind.SPEARMAN) for i in range(24)]
    client = FakeClient()
    Brain().decide(client, decision(ctx.world))
    assert any(a[1] == BuildingKind.UNIVERSITY for a in client.named('build'))


def test_optional_science_waits_until_the_first_management_units_are_funded():
    from aoe_ai.play.research import ResearchManager
    ctx = kingdom(villagers=60, centers=2, age=Age.IMPERIAL, food=1000, gold=1000,
                  researched=CASTLE + [Tech.IMPERIAL_AGE, Tech.CROSSBOWMAN, Tech.PIKEMAN])
    ctx.buildings.append(building(950, BuildingKind.CASTLE, tile=(80, 80)))
    ResearchManager().step(ctx)
    assert not ctx.client.named('research'), 'the first Steward, Vassal and General take priority'


def test_the_second_town_center_is_requested_at_thirty_villagers():
    ctx = kingdom(villagers=30, centers=1)
    ManagementManager().expand(ctx)
    assert any(a[1] == BuildingKind.TOWN_CENTER for a in ctx.client.named('build'))


def test_town_center_saving_leaves_wood_for_essential_work():
    ctx = kingdom(villagers=40, centers=1, wood=100)
    ManagementManager().expand(ctx)
    assert ctx.can_afford((0, 100, 0, 0)), 'a remote woodcutter still needs a camp'
    assert not ctx.can_afford((0, 275, 0, 100))


def test_an_established_army_waits_for_the_next_town_center_price():
    ctx = kingdom(villagers=60, centers=2, wood=100)
    from fakes import unit
    ctx.army = [unit(3000 + i, UnitKind.SPEARMAN) for i in range(ctx.config.imperial_army_min)]
    ManagementManager().expand(ctx)
    assert ctx.saving_castle, 'ongoing army spending must not consume the expansion float'
