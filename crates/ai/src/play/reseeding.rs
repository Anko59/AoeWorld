//! Prepay a small farm reserve through ordinary authoritative Mill orders
//! (`ai/aoe_ai/play/reseeding.py`).
use aoe_sdk::{BuildingKind, EntityId, Order};

use super::buildorder;
use crate::context::{Context, OrderBudgetExhausted};
use crate::roster::building_price;

pub fn queue_reseeds(ctx: &mut Context<'_>) -> Result<(), OrderBudgetExhausted> {
    if !ctx.buildings.iter().any(|b| b.kind == BuildingKind::Farm) {
        return Ok(());
    }
    let cost = building_price(BuildingKind::Farm);
    let mut reserve = cost;
    reserve[1] += buildorder::farm_wood_reserve(ctx);
    let mills: Vec<(u32, u8)> = ctx
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Mill && b.complete)
        .map(|b| (b.id, b.farm_reseeds))
        .collect();
    for (id, reseeds) in mills {
        let key = format!("reseed:{id}");
        if u32::from(reseeds) < ctx.config.farm_reseed_queue
            && !ctx.board.cooling(&key, ctx.tick)
            && ctx.can_afford(reserve)
        {
            ctx.spend(cost)?;
            let order = Order::FarmReseed {
                building: EntityId(id),
                add: true,
            };
            ctx.order_or_stop("farm_reseed", order)?;
            let wait = ctx.config.reseed_settle_seconds * ctx.hz();
            ctx.board.cool(&key, ctx.tick, wait);
            return Ok(());
        }
    }
    Ok(())
}
