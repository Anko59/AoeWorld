//! Planning tables from the public SDK rules (`ai/aoe_ai/roster.py`).
use std::sync::OnceLock;

use aoe_sdk::rules::{self, Technology};
use aoe_sdk::{Age, BuildingKind, Tech, UnitKind};

use crate::context::{Context, Lines};
use crate::view::Unit;

pub use aoe_sdk::rules::{UnitProfile, unit_profile as profile};

/// `(food, wood, gold, stone)` to raise a building.
pub fn building_price(kind: BuildingKind) -> [u32; 4] {
    let s = rules::building_cost(kind);
    [s.food, s.wood, s.gold, s.stone]
}

/// The public technology tree, read once.
pub fn technology_rows() -> &'static [Technology] {
    static ROWS: OnceLock<Vec<Technology>> = OnceLock::new();
    ROWS.get_or_init(rules::technologies)
}

/// Buildings that train at least one land soldier.
pub fn land_producers() -> &'static [BuildingKind] {
    static PRODUCERS: OnceLock<Vec<BuildingKind>> = OnceLock::new();
    PRODUCERS.get_or_init(|| {
        BuildingKind::ALL
            .into_iter()
            .filter(|b| rules::trainable(*b).into_iter().any(land_soldier))
            .collect()
    })
}

/// What a building can train now, given the age and the research done.
pub fn options(building: BuildingKind, age: Age, researched: &[Tech]) -> Vec<UnitKind> {
    rules::training_options(building, age, researched)
}

/// Living units by kind, in the order the kinds were first seen.
pub fn visible_composition(units: &[Unit], select: impl Fn(UnitKind) -> bool) -> Lines {
    let mut counts = Lines::default();
    for unit in units.iter().filter(|u| u.alive && select(u.kind)) {
        counts.add(unit.kind, 1);
    }
    counts
}

/// Fill an underrepresented line, with a bonus for known matchups.
pub fn best_recruit(
    ctx: &Context<'_>,
    choices: &[UnitKind],
    have: &Lines,
    enemies: &Lines,
    target: Option<BuildingKind>,
    preferences: Option<&[(UnitKind, f64)]>,
) -> Option<UnitKind> {
    let config = ctx.config;
    let enemy_size: u32 = enemies.values().sum();
    let (mut best, mut best_score) = (None, 0.0);
    for &kind in choices {
        if ctx.board.blocked_units.contains(&format!("{kind:?}")) || !ctx.can_afford(price(kind)) {
            continue;
        }
        let row = profile(kind);
        let mut weight = match preferences {
            Some(list) => list
                .iter()
                .find(|(line, _)| *line == row.line)
                .map_or(config.army_line_weight, |(_, w)| *w),
            None => config.army_line_weight,
        };
        if enemy_size > 0 {
            let cap = config.army_counter_bonus_cap;
            let counter: f64 = enemies
                .iter()
                .map(|(foe, n)| f64::from(n) * f64::from(counter_bonus(kind, foe).min(cap as u16)))
                .sum();
            weight *= 1.0
                + config.army_counter_weight * counter / (f64::from(enemy_size) * f64::from(cap));
        }
        if let (Some(target), true) = (target, row.siege) {
            let cap = config.army_siege_bonus_cap;
            let bonus = building_bonus(kind, target).min(cap as u16);
            weight *= 1.0 + config.army_siege_weight * f64::from(bonus) / f64::from(cap);
        }
        let score = weight / f64::from(have.get(row.line) + 1);
        if score > best_score {
            (best, best_score) = (Some(kind), score);
        }
    }
    best
}

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
