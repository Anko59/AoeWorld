//! Bounded ordinary market trades; proceeds are usable only after observation
//! (`ai/aoe_ai/play/market.py`).
use aoe_sdk::rules::market_quote;
use aoe_sdk::{BuildingKind, EntityId, Order, TradeResource};

use super::buildorder::next_stone_cost;
use crate::context::{Context, OrderBudgetExhausted};

const LOT_SIZE: i64 = 100;

/// Send one market order, then rest until the next interval.
fn trade(
    ctx: &mut Context<'_>,
    name: &str,
    order: Order,
    interval: u64,
) -> Result<(), OrderBudgetExhausted> {
    ctx.order_or_stop(name, order)?;
    ctx.board.cool("market", ctx.tick, interval);
    Ok(())
}

pub fn step(ctx: &mut Context<'_>) -> Result<(), OrderBudgetExhausted> {
    let cfg = ctx.config;
    let Some(eco) = ctx.economy.clone() else {
        return Ok(());
    };
    if ctx.board.cooling("market", ctx.tick) {
        return Ok(());
    }
    let Some(market) = ctx
        .buildings
        .iter()
        .find(|b| b.kind == BuildingKind::Market && b.complete)
        .map(|b| EntityId(b.id))
    else {
        return Ok(());
    };
    let stocks = [eco.food, eco.wood, eco.gold, eco.stone].map(i64::from);
    let reserves = [
        cfg.market_food_reserve,
        cfg.market_wood_reserve,
        cfg.market_gold_reserve,
        cfg.market_stone_reserve,
    ]
    .map(i64::from);
    let choices = [
        (1, TradeResource::Wood, eco.wood_price),
        (0, TradeResource::Food, eco.food_price),
        (3, TradeResource::Stone, eco.stone_price),
    ];
    let bulk = ctx.castle();
    let spent = ctx.spent.map(i64::from);
    let gold = i64::from(eco.gold) - spent[2];
    let interval = cfg.market_interval_seconds * ctx.hz();
    let stone_goal = i64::from(next_stone_cost(ctx));
    let stone_short = i64::from(eco.stone) - spent[3] < stone_goal;
    let gold_reserve = i64::from(cfg.market_gold_reserve);
    if stone_short && ctx.castle() {
        let quote = market_quote(eco.stone_price, true);
        if gold >= i64::from(cfg.market_stone_gold_floor) + i64::from(quote)
            && ctx.try_spend([0, 0, quote, 0])?
        {
            let order = Order::MarketBuy {
                building: market,
                resource: TradeResource::Stone,
            };
            return trade(ctx, "market_buy", order, interval);
        }
    }
    if (if bulk { gold } else { i64::from(eco.gold) }) < gold_reserve {
        let surplus: [i64; 4] = std::array::from_fn(|i| stocks[i] - reserves[i]);
        let available: [i64; 4] = std::array::from_fn(|i| surplus[i] - spent[i]);
        let offered = if bulk { available } else { surplus };
        let mut best = choices[0];
        for choice in &choices[1..] {
            if offered[choice.0] > offered[best.0] {
                best = *choice;
            }
        }
        let (mut i, mut kind, price) = best;
        let mut lots;
        if bulk {
            let quote = i64::from(market_quote(price, false));
            let needed = (gold_reserve - gold + quote - 1).div_euclid(quote);
            lots = available[i]
                .div_euclid(LOT_SIZE)
                .min(needed)
                .min(i64::from(cfg.market_max_sale_lots));
        } else {
            lots = i64::from(available[i] > LOT_SIZE);
        }
        if lots <= 0 && bulk && gold < i64::from(cfg.market_stone_gold_floor) {
            // A stalled market can release stone held above both the next
            // construction bill and a buffer. Preserve the buffer on the buy side.
            let floor = i64::from(cfg.market_emergency_stone_reserve).max(stone_goal + LOT_SIZE);
            lots = i64::from(cfg.market_emergency_sale_lots)
                .min((i64::from(eco.stone) - spent[3] - floor).div_euclid(LOT_SIZE));
            (i, kind) = (3, TradeResource::Stone);
        }
        if lots <= 0 {
            return Ok(());
        }
        let mut cost = [0u32; 4];
        cost[i] = (lots * LOT_SIZE) as u32;
        ctx.spend(cost)?;
        let order = if lots == 1 {
            Order::MarketSell {
                building: market,
                resource: kind,
            }
        } else {
            Order::MarketTrade {
                building: market,
                resource: kind,
                buy: false,
                lots: lots as u32,
            }
        };
        let name = if lots == 1 {
            "market_sell"
        } else {
            "market_trade"
        };
        trade(ctx, name, order, interval)?;
    } else if gold > gold_reserve {
        for (i, kind, price) in &choices[..2] {
            if stocks[*i] < reserves[*i] / 2 {
                let cost = [0, 0, market_quote(*price, true), 0];
                if ctx.try_spend(cost)? {
                    let order = Order::MarketBuy {
                        building: market,
                        resource: *kind,
                    };
                    trade(ctx, "market_buy", order, interval)?;
                }
                return Ok(());
            }
        }
    }
    Ok(())
}
