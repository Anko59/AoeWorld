//! How the villagers are meant to split across food, wood, gold and stone.
use aoe_sdk::{Tech, UnitTask};

use super::{FOOD, GOLD, STONE, WOOD};
use crate::context::Context;
use crate::play::buildorder;
use crate::roster::research_price;

/// Villagers on each resource: hunters and farmers count as food.
pub fn counts(ctx: &Context<'_>) -> [u32; 4] {
    let mut counts = [0; 4];
    for v in &ctx.villagers {
        if matches!(v.task, UnitTask::Hunting | UnitTask::Farming) {
            counts[FOOD] += 1;
        } else if let Some(kind) = v.gathering {
            counts[crate::blackboard::slot(kind)] += 1;
        }
    }
    counts
}

/// The wanted share of villagers per resource for this decision.
pub fn shares(ctx: &Context<'_>) -> [f64; 4] {
    let cfg = ctx.config;
    let Some(eco) = &ctx.economy else {
        return [0.0; 4];
    };
    let villagers = ctx.villagers.len();
    let mut shares = if ctx.feudal() {
        cfg.feudal_shares
    } else {
        cfg.dark_shares
    };
    // Saving 500 food for Feudal Age: lean on food.
    if !ctx.feudal() && villagers as i64 >= i64::from(cfg.feudal_villagers) - 4 {
        shares[FOOD] += 0.15;
        shares[WOOD] -= 0.10;
        shares[GOLD] = (shares[GOLD] - 0.05).max(0.0);
    }
    if buildorder::wants_castle_food(ctx)
        && eco.gold >= research_price(Tech::CastleAge)[2] + research_price(Tech::Crossbowman)[2]
    {
        shares[FOOD] += shares[GOLD];
        shares[GOLD] = 0.0;
    }
    if buildorder::wants_imperial_food(ctx) {
        shares = cfg.imperial_shares;
    }
    if ctx.tick as i64 - ctx.board.saving_tick <= 3 * ctx.hz() as i64 {
        shares = [0.45, 0.25, 0.30, 0.0];
    } else if ctx.castle() && !buildorder::wants_imperial_food(ctx) {
        shares = cfg.castle_shares;
    }
    if ctx.castle() {
        let stone_goal = buildorder::next_stone_cost(ctx);
        let gold_gap = 1.0 - f64::from(eco.gold) / f64::from(cfg.market_gold_reserve.max(1));
        let stone_gap = if stone_goal > 0 {
            cfg.castle_stone_share * (1.0 - f64::from(eco.stone) / f64::from(stone_goal)).max(0.0)
        } else {
            0.0
        };
        let shortages = [
            (GOLD, cfg.castle_gold_boost * gold_gap.max(0.0)),
            (STONE, stone_gap),
        ];
        // Keep both food and wood available for villagers, farms and camps.
        let spare = [FOOD, WOOD].map(|k| (k, (shares[k] - cfg.castle_worker_floors[k]).max(0.0)));
        let available: f64 = spare.iter().map(|(_, a)| a).sum();
        let requested: f64 = shortages.iter().map(|(_, a)| a).sum();
        if available != 0.0 && requested != 0.0 {
            let shifted = available.min(requested);
            for (kind, amount) in spare {
                shares[kind] -= shifted * amount / available;
            }
            for (kind, amount) in shortages {
                shares[kind] += shifted * amount / requested;
            }
        }
    }
    if villagers < 10 {
        shares[GOLD] = 0.0;
        shares[STONE] = 0.0;
    }
    shares
}
