"""A market must be affordable before it can trade an exhausted treasury back up."""
from aoeworld import BuildingKind

from aoe_ai import Brain, Config
from fakes import FakeWorld
from test_imperial import castle_base, castle_economy
from test_logic import play


def marketless_castle(wood):
    units, buildings = castle_base(villagers=27, soldiers=30)
    return FakeWorld(units, buildings, eco=castle_economy(
        food=700, wood=wood, gold=700, stone=850, population=58))


def test_a_first_market_precedes_discretionary_wood_spending():
    brain = Brain(Config(castles=0, stewards=0, vassals=0, generals=0))
    brain.economy.work = lambda ctx: ctx.try_spend((0, 60, 0, 0))

    _, client = play(marketless_castle(225), brain=brain)

    assert any(args[1] == BuildingKind.MARKET for args in client.named("build"))


def test_an_unfunded_first_market_keeps_a_working_wood_float():
    brain = Brain(Config(castles=0, stewards=0, vassals=0, generals=0))
    wood_left = []
    brain.economy.work = lambda ctx: wood_left.append(ctx.economy.wood - ctx.spent[1])

    _, client = play(marketless_castle(180), brain=brain)

    assert wood_left == [brain.config.expansion_wood_float]
    assert not any(args[1] == BuildingKind.MARKET for args in client.named("build"))


def test_emergency_gold_does_not_consume_the_expansion_stone_buffer():
    from aoe_ai.play.market import MarketManager
    from fakes import building
    from test_kingdom import kingdom

    ctx = kingdom(villagers=28, centers=1, gold=0, stone=450, food=250, wood=300)
    ctx.buildings += [building(950, BuildingKind.MARKET), building(951, BuildingKind.CASTLE)]
    ctx.economy.food_price = ctx.economy.wood_price = ctx.economy.stone_price = 100
    MarketManager().step(ctx)

    assert not ctx.client.named("market_sell")
    assert not ctx.client.named("market_trade")
