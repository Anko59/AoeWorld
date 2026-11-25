//! Management units and the buildings they need
//! (`ai/aoe_ai/play/management.py`).
//!
//! Only the price of the first management units is here for now: research
//! protects it from optional science. The expansion, castles, seating and
//! generals come with the management task.
use aoe_sdk::rules::unit_train_seconds;
use aoe_sdk::{BuildingKind, UnitKind};

use super::fortification::threat::threatened;
use crate::context::Context;
use crate::roster::price;
use crate::view::Building;

fn garrisoned(building: &Building, kind: UnitKind) -> bool {
    building.garrison.iter().any(|entry| entry.1 == kind)
}

/// Roaming Vassals and occupied frontier seats; rear seats cannot defend here.
fn available_vassals(ctx: &Context<'_>) -> usize {
    ctx.units
        .iter()
        .filter(|u| u.kind == UnitKind::Vassal)
        .count()
        + ctx
            .buildings
            .iter()
            .filter(|b| garrisoned(b, UnitKind::Vassal) && threatened(ctx, b.center()))
            .count()
}

fn ready_for_stewards(ctx: &Context<'_>) -> bool {
    let cfg = ctx.config;
    let workers = ctx.villagers.len();
    cfg.stewards > 0
        && (workers >= cfg.steward_min_villagers as usize
            || (workers >= cfg.expand_villagers as usize
                && ctx.army.len() + ctx.led.len() >= cfg.imperial_army_min as usize
                && ctx.count(BuildingKind::Market, true) > 0))
}

/// Protect the first available manager of each enabled kind from science.
pub fn funding_reserve(ctx: &Context<'_>) -> [u32; 4] {
    let cfg = ctx.config;
    let mut result = [0; 4];
    if !cfg.save_for_management || !ctx.castle() {
        return result;
    }
    let vassals = cfg.vassals > 0
        && ctx
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Castle)
            .any(|b| threatened(ctx, b.center()));
    let kinds = [
        (
            ready_for_stewards(ctx),
            UnitKind::Steward,
            BuildingKind::TownCenter,
        ),
        (vassals, UnitKind::Vassal, BuildingKind::Castle),
        (cfg.generals > 0, UnitKind::General, BuildingKind::Castle),
    ];
    for (enabled, kind, house) in kinds {
        if !enabled || ctx.count(house, true) == 0 {
            continue;
        }
        let mut present = ctx.units.iter().any(|u| u.kind == kind)
            || ctx.buildings.iter().any(|b| garrisoned(b, kind));
        if kind == UnitKind::Vassal {
            present = available_vassals(ctx) > 0;
        }
        let window = (u64::from(unit_train_seconds(kind)) + 20) * ctx.hz();
        let pending = ctx
            .board
            .producing
            .get(&format!("{kind:?}"))
            .is_some_and(|orders| {
                orders
                    .iter()
                    .any(|(tick, _)| ctx.tick.saturating_sub(*tick) < window)
            });
        if !present && !pending {
            for (total, cost) in result.iter_mut().zip(price(kind)) {
                *total += cost;
            }
        }
    }
    result
}
