"""Bounded ordinary market trades; proceeds are usable only after observation."""
from aoeworld import BuildingKind, TradeResource, market_quote


class MarketManager:
    def step(self, ctx):
        cfg, eco = ctx.config, ctx.economy
        if eco is None or ctx.board.cooling("market", ctx.tick):
            return
        market = next((b for b in ctx.buildings if b.kind == BuildingKind.MARKET and b.complete), None)
        if market is None:
            return
        stocks = (eco.food, eco.wood, eco.gold, eco.stone)
        reserves = (cfg.market_food_reserve, cfg.market_wood_reserve, cfg.market_gold_reserve, cfg.market_stone_reserve)
        choices = ((1, TradeResource.WOOD, eco.wood_price),
                   (0, TradeResource.FOOD, eco.food_price),
                   (3, TradeResource.STONE, eco.stone_price))
        if eco.gold < cfg.market_gold_reserve:
            i, kind, price = max(choices, key=lambda c: stocks[c[0]] - reserves[c[0]])
            if stocks[i] - ctx.spent[i] > reserves[i] + 100:
                cost = [0, 0, 0, 0]
                cost[i] = 100
                ctx.spend(tuple(cost))
                ctx.order("market_sell", market.id, kind)
                ctx.board.cool("market", ctx.tick, cfg.market_interval_seconds * ctx.world.tick_hz)
        elif eco.gold - ctx.spent[2] > cfg.market_gold_reserve:
            for i, kind, price in choices:
                if stocks[i] < reserves[i] // 2:
                    cost = (0, 0, market_quote(price, True), 0)
                    if ctx.try_spend(cost):
                        ctx.order("market_buy", market.id, kind)
                        ctx.board.cool("market", ctx.tick, cfg.market_interval_seconds * ctx.world.tick_hz)
                    return
