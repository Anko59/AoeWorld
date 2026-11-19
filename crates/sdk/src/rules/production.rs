//! Public planning views of the simulation's production and combat tables.
use super::{native_building, native_tech, native_unit, public_age, public_unit};
use crate::{Age, BuildingKind, Tech, UnitKind};
use aoe_simulation as sim;

pub(super) fn line(kind: sim::UnitKind) -> UnitKind {
    if kind.is_trebuchet() {
        return UnitKind::Trebuchet;
    }
    let endpoint = kind.upgraded(u128::MAX);
    public_unit(
        sim::UnitKind::ALL
            .into_iter()
            .find(|candidate| {
                candidate.required_tech().is_none() && candidate.upgraded(u128::MAX) == endpoint
            })
            .unwrap_or(kind),
    )
}

/// Trainable identities after resolving upgrades, age and tech requirements.
/// The caller supplies public rules inputs from its own economy view.
pub fn training_options(building: BuildingKind, age: Age, researched: &[Tech]) -> Vec<UnitKind> {
    let age = match age {
        Age::Dark => sim::Age::Dark,
        Age::Feudal => sim::Age::Feudal,
        Age::Castle => sim::Age::Castle,
        Age::Imperial => sim::Age::Imperial,
    };
    let researched = researched
        .iter()
        .fold(0, |mask, &tech| mask | native_tech(tech).bit());
    sim::trainable_units(native_building(building))
        .iter()
        .copied()
        .filter(|&kind| {
            kind.upgraded(researched) == kind
                && kind.required_age() <= age
                && kind
                    .required_tech()
                    .is_none_or(|tech| researched & tech.bit() != 0)
        })
        .map(public_unit)
        .collect()
}

/// Stable line identity and base traits of a unit in the public combat table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitProfile {
    pub line: UnitKind,
    pub age: Age,
    pub armed: bool,
    pub naval: bool,
    pub siege: bool,
}

pub fn unit_profile(kind: UnitKind) -> UnitProfile {
    let kind = native_unit(kind);
    UnitProfile {
        line: line(kind),
        age: public_age(kind.required_age()),
        armed: sim::unit_combat(kind).attack > 0,
        naval: kind.is_naval(),
        siege: kind.is_siege(),
    }
}

/// Base class bonus against a unit. Does not read either player's research.
pub fn unit_counter_bonus(attacker: UnitKind, target: UnitKind) -> u16 {
    sim::class_bonus(native_unit(attacker), native_unit(target))
}

/// Base class bonus against a building before its owner's Masonry.
pub fn unit_building_bonus(attacker: UnitKind, target: BuildingKind) -> u16 {
    sim::unit_building_bonus(native_unit(attacker), native_building(target))
}

pub(super) fn technology_units(
    row: &sim::TechRules,
) -> (Vec<UnitKind>, Vec<UnitKind>, Vec<UnitKind>) {
    let tech = row.tech;
    let target = match row.effect {
        sim::TechEffect::UnitSpeed { target, .. } | sim::TechEffect::UnitHp { target, .. } => {
            Some(target)
        }
        _ => None,
    };
    let mut upgrades = Vec::new();
    let mut unlocks = Vec::new();
    let mut improves = Vec::new();
    for kind in sim::UnitKind::ALL {
        if kind.upgraded(tech.bit()) != kind {
            let family = line(kind);
            if !upgrades.contains(&family) {
                upgrades.push(family);
            }
        }
        if kind.required_tech() == Some(tech) {
            unlocks.push(public_unit(kind));
        }
        if target.is_some_and(|target| target.includes(kind)) {
            improves.push(public_unit(kind));
        }
    }
    (upgrades, unlocks, improves)
}
