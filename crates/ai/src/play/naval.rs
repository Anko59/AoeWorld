//! Water food and the reach check of every food order (`ai/aoe_ai/play/naval.py`).
//! The fishing fleet itself (`NavalManager`) is a later task.
use aoe_sdk::ResourceKind;

use crate::context::Context;
use crate::geometry::dist;
use crate::view::{Node, Unit};

pub fn water_food(ctx: &Context<'_>, node: &Node) -> bool {
    node.kind == ResourceKind::Food && ctx.world.passable(node.tile()) == Some(false)
}

/// Fish are reached from an adjacent bank, never by a land route into water.
pub fn food_reachable(ctx: &mut Context<'_>, villager: &Unit, node: &Node) -> bool {
    if !water_food(ctx, node) {
        return ctx.land_route(villager.tile(), node.tile()).is_some();
    }
    let (x, y) = node.tile();
    let mut banks = Vec::new();
    for dx in -1..=1 {
        for dy in -1..=1 {
            let bank = (x + dx, y + dy);
            if (dx != 0 || dy != 0) && ctx.world.passable(bank) == Some(true) {
                banks.push(bank);
            }
        }
    }
    banks.sort_by(|a, b| dist(*a, villager.tile()).total_cmp(&dist(*b, villager.tile())));
    banks
        .into_iter()
        .any(|bank| ctx.land_route(villager.tile(), bank).is_some())
}
