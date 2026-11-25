//! Planning tables from the public SDK rules (`ai/aoe_ai/roster.py`).
use aoe_sdk::rules;
use aoe_sdk::{Age, BuildingKind, Tech, UnitKind};

pub use aoe_sdk::rules::{UnitProfile, unit_profile as profile};

/// `(food, wood, gold, stone)` to queue a unit.
pub fn price(kind: UnitKind) -> [u32; 4] {
    let s = rules::unit_cost(kind);
    [s.food, s.wood, s.gold, s.stone]
}

/// `(food, wood, gold, stone)` to research a technology.
pub fn research_price(tech: Tech) -> [u32; 4] {
    let s = rules::tech_cost(tech);
    [s.food, s.wood, s.gold, s.stone]
}

pub fn counter_bonus(attacker: UnitKind, target: UnitKind) -> u16 {
    rules::unit_counter_bonus(attacker, target)
}

pub fn building_bonus(attacker: UnitKind, target: BuildingKind) -> u16 {
    rules::unit_building_bonus(attacker, target)
}

pub fn age_rank(age: Age) -> usize {
    match age {
        Age::Dark => 0,
        Age::Feudal => 1,
        Age::Castle => 2,
        Age::Imperial => 3,
    }
}

/// Armed land units the AI commands: not the starting Scout, which explores,
/// and not Generals, which have their own production and orders.
pub fn land_soldier(kind: UnitKind) -> bool {
    let row = profile(kind);
    row.armed
        && !row.naval
        && !matches!(
            kind,
            UnitKind::Villager | UnitKind::Scout | UnitKind::General
        )
}

pub fn warship(kind: UnitKind) -> bool {
    let row = profile(kind);
    row.naval && row.armed
}
