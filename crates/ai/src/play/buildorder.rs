//! The build order: which stage the game is in and what it asks for next
//! (`ai/aoe_ai/play/buildorder.py`).
//!
//! Dark Age: boom (villagers without pause, a Lumber Camp and a Mill), Loom,
//! then at about twenty villagers save 500 food and click Feudal Age. Feudal
//! Age: Barracks, Archery Range, Blacksmith, the economy upgrades and a mixed
//! army. The managers only *execute* this plan.
use aoe_sdk::{BuildingKind, Tech};

use crate::context::Context;
use crate::roster::building_price;

/// At least two different kinds of these must stand before Feudal Age.
pub const FEUDAL_REQUIREMENT: [BuildingKind; 5] = [
    BuildingKind::Barracks,
    BuildingKind::Mill,
    BuildingKind::LumberCamp,
    BuildingKind::MiningCamp,
    BuildingKind::Dock,
];
pub const CASTLE_REQUIREMENT: [BuildingKind; 4] = [
    BuildingKind::ArcheryRange,
    BuildingKind::Stable,
    BuildingKind::Blacksmith,
    BuildingKind::Market,
];
/// At least two different kinds of these must stand before Imperial Age.
pub const IMPERIAL_REQUIREMENT: [BuildingKind; 4] = [
    BuildingKind::Castle,
    BuildingKind::University,
    BuildingKind::Monastery,
    BuildingKind::SiegeWorkshop,
];
/// The two the AI raises: cheap wood buildings (the Siege Workshop needs the Blacksmith).
pub const IMPERIAL_BUILDINGS: [BuildingKind; 2] =
    [BuildingKind::University, BuildingKind::SiegeWorkshop];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Dark,
    Advance,
    Feudal,
    CastleAdvance,
    Castle,
    ImperialAdvance,
    Imperial,
}

/// A building that must be finished before another can be placed.
pub fn prerequisite(kind: BuildingKind) -> Option<BuildingKind> {
    match kind {
        BuildingKind::ArcheryRange | BuildingKind::Stable => Some(BuildingKind::Barracks),
        BuildingKind::SiegeWorkshop => Some(BuildingKind::Blacksmith),
        _ => None,
    }
}

/// A counter of the statistics board (0 when never set).
pub fn stat(ctx: &Context<'_>, name: &str) -> u64 {
    ctx.board.stats.get(name).copied().unwrap_or(0)
}

/// How many distinct finished kinds of `among` stand.
fn finished_kinds(ctx: &Context<'_>, among: &[BuildingKind]) -> usize {
    among
        .iter()
        .filter(|kind| ctx.buildings.iter().any(|b| b.complete && b.kind == **kind))
        .count()
}

/// Distinct finished kinds that count toward Feudal Age.
pub fn requirement_kinds(ctx: &Context<'_>) -> usize {
    finished_kinds(ctx, &FEUDAL_REQUIREMENT)
}

pub fn castle_requirement_kinds(ctx: &Context<'_>) -> usize {
    finished_kinds(ctx, &CASTLE_REQUIREMENT)
}

pub fn imperial_requirement_kinds(ctx: &Context<'_>) -> usize {
    finished_kinds(ctx, &IMPERIAL_REQUIREMENT)
}

/// The Castle Age economy and army the climb to Imperial Age is built on.
pub fn imperial_ready(ctx: &Context<'_>) -> bool {
    ctx.castle()
        && !ctx.imperial()
        && ctx.villagers.len() >= ctx.config.imperial_villagers as usize
        && stat(ctx, "army_peak") >= u64::from(ctx.config.imperial_army_min)
}

/// Save the age's food and gold while the Castle army and buildings finish.
pub fn wants_imperial_food(ctx: &Context<'_>) -> bool {
    imperial_ready(ctx)
}

/// Kinds standing or planned (placed, maybe not yet visible).
fn placed_kinds(ctx: &Context<'_>) -> Vec<BuildingKind> {
    let mut placed: Vec<BuildingKind> = Vec::new();
    for kind in ctx
        .buildings
        .iter()
        .map(|b| b.kind)
        .chain(ctx.board.plans.iter().map(|p| p.kind))
    {
        if !placed.contains(&kind) {
            placed.push(kind);
        }
    }
    placed
}

fn placed_among(placed: &[BuildingKind], among: &[BuildingKind]) -> usize {
    among.iter().filter(|k| placed.contains(k)).count()
}

/// Protect the next prerequisite's budget as soon as Feudal begins.
///
/// Paid foundations and pending orders already reserve their own cost. Rich
/// stockpiles can still feed farms and soldiers above this bounded reserve.
pub fn castle_prerequisite_wood(ctx: &Context<'_>) -> u32 {
    if !ctx.feudal() || ctx.castle() {
        return 0;
    }
    let reserve = ctx.config.wood_reserve;
    if unpaid_age_up_barracks(ctx) {
        return building_price(BuildingKind::Barracks)[1] + reserve;
    }
    if castle_requirement_kinds(ctx) >= 2 || emergency_defence(ctx) {
        return 0;
    }
    let placed = placed_kinds(ctx);
    if placed_among(&placed, &CASTLE_REQUIREMENT) >= 2 {
        return 0;
    }
    [BuildingKind::ArcheryRange, BuildingKind::Blacksmith]
        .into_iter()
        .filter(|k| !placed.contains(k))
        .map(|k| building_price(k)[1])
        .min()
        .unwrap_or(0)
        + reserve
}

pub fn stage(ctx: &Context<'_>) -> Stage {
    let cfg = ctx.config;
    if ctx.imperial() {
        return Stage::Imperial;
    }
    if ctx.castle() {
        if imperial_ready(ctx) && imperial_requirement_kinds(ctx) >= 2 {
            return Stage::ImperialAdvance;
        }
        return Stage::Castle;
    }
    if ctx.feudal() {
        let ready = castle_requirement_kinds(ctx) >= 2;
        return if ready
            && ctx.villagers.len() >= cfg.castle_villagers as usize
            && stat(ctx, "army_peak") >= u64::from(cfg.castle_army_min)
        {
            Stage::CastleAdvance
        } else {
            Stage::Feudal
        };
    }
    if ctx.villagers.len() >= cfg.feudal_villagers as usize && requirement_kinds(ctx) >= 2 {
        return Stage::Advance;
    }
    Stage::Dark
}

pub fn emergency_defence(ctx: &Context<'_>) -> bool {
    ctx.board.raid_until > ctx.tick as i64
        && ctx.army.len() + ctx.led.len() < ctx.config.castle_army_min as usize
}

/// Prepare the age's food while the Feudal army finishes forming.
pub fn wants_castle_food(ctx: &Context<'_>) -> bool {
    ctx.feudal() && !ctx.castle() && ctx.villagers.len() >= ctx.config.castle_villagers as usize
}

pub fn wants_loom(ctx: &Context<'_>) -> bool {
    ctx.villagers.len() >= ctx.config.loom_villagers as usize
}

fn researching_age(ctx: &Context<'_>, tech: Tech) -> bool {
    ctx.center
        .as_ref()
        .is_some_and(|c| c.researching == Some(tech))
}

fn barracks_placed(ctx: &Context<'_>) -> bool {
    ctx.buildings
        .iter()
        .any(|b| b.kind == BuildingKind::Barracks)
        || ctx
            .board
            .plans
            .iter()
            .any(|p| p.kind == BuildingKind::Barracks)
}

/// The age-up's first Barracks must precede discretionary wood drop-offs.
pub fn unpaid_age_up_barracks(ctx: &Context<'_>) -> bool {
    ((ctx.feudal() && !ctx.castle()) || researching_age(ctx, Tech::FeudalAge))
        && !barracks_placed(ctx)
}

/// Food expansion must leave wood for houses and the first Barracks.
///
/// Start saving when the villagers are ready to advance, even if food is still
/// accumulating. A foundation or pending order has already been paid. Free
/// food and existing farms can keep feeding us while wood accumulates.
pub fn farm_wood_reserve(ctx: &Context<'_>) -> u32 {
    let advancing = ctx.feudal()
        || ctx.villagers.len() >= ctx.config.feudal_villagers as usize
        || researching_age(ctx, Tech::FeudalAge);
    let barracks = if advancing && !barracks_placed(ctx) {
        building_price(BuildingKind::Barracks)[1]
    } else {
        0
    };
    ctx.config.wood_reserve + barracks
}

pub fn imperial_prerequisite_wood(ctx: &Context<'_>) -> u32 {
    if !imperial_ready(ctx) || imperial_requirement_kinds(ctx) >= 2 {
        return 0;
    }
    let placed = placed_kinds(ctx);
    if placed_among(&placed, &IMPERIAL_REQUIREMENT) >= 2 {
        return 0;
    }
    IMPERIAL_BUILDINGS
        .into_iter()
        .filter(|k| !placed.contains(k))
        .map(|k| building_price(k)[1])
        .min()
        .unwrap_or(0)
        + ctx.config.wood_reserve
}

pub fn wanted_centers(ctx: &Context<'_>) -> usize {
    let cfg = ctx.config;
    (cfg.town_centers as usize)
        .max(1 + ctx.villagers.len() / cfg.expand_workers_per_center as usize)
}

pub fn villager_target(ctx: &Context<'_>) -> usize {
    let cfg = ctx.config;
    (cfg.base_villagers as usize)
        .max(ctx.count(BuildingKind::TownCenter, true) * cfg.workers_per_center as usize)
}

pub fn wanted_castles(ctx: &Context<'_>) -> usize {
    let castles = ctx.config.castles as usize;
    if castles == 0 {
        return 0;
    }
    castles.max(ctx.count(BuildingKind::TownCenter, true).div_ceil(2))
}

pub fn planned_count(ctx: &Context<'_>, kind: BuildingKind) -> usize {
    ctx.count(kind, false) + ctx.board.plans.iter().filter(|p| p.kind == kind).count()
}

/// The next structure worth stockpiling or trading stone for.
pub fn next_stone_cost(ctx: &Context<'_>) -> u32 {
    let centers = planned_count(ctx, BuildingKind::TownCenter);
    if (centers == 0 && !ctx.villagers.is_empty())
        || (ctx.castle()
            && ctx.villagers.len() >= ctx.config.expand_villagers as usize
            && centers < wanted_centers(ctx))
    {
        return building_price(BuildingKind::TownCenter)[3];
    }
    if ctx.castle() && planned_count(ctx, BuildingKind::Castle) < wanted_castles(ctx) {
        return building_price(BuildingKind::Castle)[3];
    }
    0
}

pub fn wants_urgent_market(ctx: &Context<'_>) -> bool {
    let Some(eco) = &ctx.economy else {
        return false;
    };
    if !ctx.castle() || ctx.villagers.len() < 24 {
        return false;
    }
    let (gold, stone) = (eco.gold, eco.stone);
    let next_bill = next_stone_cost(ctx);
    let center_shortage =
        next_bill == building_price(BuildingKind::TownCenter)[3] && stone < next_bill;
    let established = ctx.army.len() + ctx.led.len() >= ctx.config.imperial_army_min as usize;
    gold < ctx.config.market_stone_gold_floor
        || center_shortage
        || (established && (gold < ctx.config.market_gold_reserve || stone < next_bill))
}

/// (kind, how many) in the order they should appear; Feudal Age only.
///
/// Dark Age drop-offs (Lumber Camp, Mill, mining camps) follow the resources,
/// see the constructor.
pub fn wanted_buildings(ctx: &Context<'_>) -> Vec<(BuildingKind, usize)> {
    let cfg = ctx.config;
    let villagers = ctx.villagers.len();
    if !ctx.feudal() {
        // The Barracks is a Dark Age building: raise it while the age researches.
        if researching_age(ctx, Tech::FeudalAge) {
            return vec![(BuildingKind::Barracks, 1)];
        }
        return Vec::new();
    }
    if unpaid_age_up_barracks(ctx) {
        return vec![(BuildingKind::Barracks, 1)];
    }
    let centers = ctx.count(BuildingKind::TownCenter, true);
    // Finish the Blacksmith before expanding production so the second Castle
    // prerequisite does not compete with another Barracks for construction wood.
    let mut wants = vec![
        (BuildingKind::Barracks, 1),
        (BuildingKind::ArcheryRange, 1),
        (BuildingKind::Blacksmith, 1),
    ];
    let market_needed = villagers >= 24;
    let urgent_market = wants_urgent_market(ctx);
    if urgent_market {
        wants.push((BuildingKind::Market, 1));
    }
    if ctx.castle() || researching_age(ctx, Tech::CastleAge) {
        wants.insert(2, (BuildingKind::Stable, (centers / 2).max(1)));
    }
    // The Imperial Age buildings come before the second Barracks and Range:
    // they are the age's prerequisite, the extra production is not.
    if ctx.castle() && villagers + 6 >= cfg.imperial_villagers as usize {
        wants.extend(IMPERIAL_BUILDINGS.into_iter().map(|kind| (kind, 1)));
    }
    if villagers >= 22 {
        wants.push((
            BuildingKind::Barracks,
            (cfg.max_barracks as usize).max(centers),
        ));
    }
    if villagers >= 26 {
        wants.push((
            BuildingKind::ArcheryRange,
            (cfg.max_ranges as usize).max(centers),
        ));
    }
    if market_needed && !urgent_market {
        wants.push((BuildingKind::Market, 1));
    }
    wants
}
