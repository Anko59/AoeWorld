"""Bounded ordinary market trades; proceeds are usable only after observation."""
from aoeworld import BuildingKind, TradeResource, market_quote


LOT_SIZE = 100


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
        bulk = ctx.castle
        gold = eco.gold - ctx.spent[2]
        if (gold if bulk else eco.gold) < cfg.market_gold_reserve:
            surplus = tuple(stock - reserve for stock, reserve in zip(stocks, reserves))
            available = tuple(stock - spent for stock, spent in zip(surplus, ctx.spent))
            offered = available if bulk else surplus
            i, kind, price = max(choices, key=lambda c: offered[c[0]])
            if bulk:
                quote = market_quote(price, False)
                needed = (cfg.market_gold_reserve - gold + quote - 1) // quote
                lots = min(available[i] // LOT_SIZE, needed, cfg.market_max_sale_lots)
            else:
                lots = int(available[i] > LOT_SIZE)
            if lots <= 0:
                return
            cost = [0, 0, 0, 0]
            cost[i] = lots * LOT_SIZE
            ctx.spend(tuple(cost))
            if lots == 1:
                ctx.order("market_sell", market.id, kind)
            else:
                ctx.order("market_trade", market.id, kind, False, lots)
            ctx.board.cool("market", ctx.tick, cfg.market_interval_seconds * ctx.world.tick_hz)
        elif gold > cfg.market_gold_reserve:
            for i, kind, price in choices:
                if stocks[i] < reserves[i] // 2:
                    cost = (0, 0, market_quote(price, True), 0)
                    if ctx.try_spend(cost):
                        ctx.order("market_buy", market.id, kind)
                        ctx.board.cool("market", ctx.tick, cfg.market_interval_seconds * ctx.world.tick_hz)
                    return
