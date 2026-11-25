//! Management units and the buildings they need
//! (`ai/aoe_ai/play/management.py`).
//!
//! * Castles (stone) host Vassals (defence, towers, walls, roads) and Generals.
//! * A Steward garrisoned in a Town Center manages the villagers of a 100-tile
//!   domain and builds Houses; one per Town Center.
//! * A General leads up to 100 soldiers; with his Train button pressed he buys
//!   them himself from the production buildings in his 50-tile sphere.
//! * More Town Centers train more villagers and widen the Steward domains.
//!
//! Prices are saved ahead of army spending (`save_for_management`): a Steward
//! costs 1,000 food and 1,000 gold, a General 800 and 800.
//!
//! The manager is split by concern: `building` (Town Centers and Castles),
//! `units` (Stewards and Vassals) and `general` (Generals and their orders).
mod building;
mod general;
mod units;

use aoe_sdk::rules::unit_train_seconds;
use aoe_sdk::{BuildingKind, UnitKind};

use super::fortification::threat::threatened;
use crate::context::{Context, OrderBudgetExhausted};
use crate::roster::price;
use crate::view::Building;

type Done = Result<(), OrderBudgetExhausted>;

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

#[derive(Clone, Copy, Debug, Default)]
pub struct ManagementManager {
    /// Set by `place` when a reachability check had no budget to answer.
    unanswered: bool,
}

impl ManagementManager {
    /// One decision.
    pub fn step(&mut self, ctx: &mut Context<'_>) -> Done {
        let cfg = ctx.config;
        if ctx.home.is_none() || ctx.economy.is_none() {
            return Ok(());
        }
        if cfg.castles == 0
            && cfg.stewards == 0
            && cfg.vassals == 0
            && cfg.generals == 0
            && cfg.town_centers <= 1
        {
            return Ok(());
        }
        self.expand(ctx)?;
        self.castles(ctx)?;
        self.seat(ctx, UnitKind::Steward, BuildingKind::TownCenter)?;
        self.seat(ctx, UnitKind::Vassal, BuildingKind::Castle)?;
        self.recruit(ctx)?;
        self.generals(ctx)?;
        self.settings(ctx)?;
        self.defenders(ctx)
    }
}
