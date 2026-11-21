//! Naval technologies after the Naval Warfare Overhaul (DE update 169123).
//! Costs and times: SiegeEngineers/aoe2techtree data.json (DE update 188329).
//! Ages and effects: docs/navy.md, with its sources and stated assumptions.
use super::super::{BuildingKind, Stockpile, UnitKind as U};
use super::rule_effect::{ArmorClass, RuleEffect};
use super::table::*;

const fn cost(food: u32, wood: u32, gold: u32) -> Stockpile {
    Stockpile {
        food,
        wood,
        gold,
        stone: 0,
    }
}

/// Medium Warships: Galley, Fire Galley and Hulk gain their second tier.
pub(super) const MEDIUM: &[(U, U)] = &[
    (U::Galley, U::WarGalley),
    (U::FireGalley, U::FireShip),
    (U::Hulk, U::WarHulk),
];
/// Heavy Warships: War Galley, Fire Ship and War Hulk gain their third tier.
const HEAVY: &[(U, U)] = &[
    (U::WarGalley, U::Galleon),
    (U::FireShip, U::FastFireShip),
    (U::WarHulk, U::Carrack),
];
const DEMOLITION: &[(U, U)] = &[(U::DemolitionRaft, U::DemolitionShip)];
const HEAVY_DEMOLITION: &[(U, U)] = &[(U::DemolitionShip, U::HeavyDemolitionShip)];
const ELITE_CANNON: &[(U, U)] = &[(U::CannonGalleon, U::EliteCannonGalleon)];

const fn rule(
    tech: Tech,
    building: BuildingKind,
    age: Age,
    cost: Stockpile,
    seconds: u32,
    requires: TechRequirement,
    effect: TechEffect,
) -> TechRules {
    TechRules {
        age,
        tech,
        building,
        cost,
        seconds,
        requires,
        effect,
    }
}

use BuildingKind::{Dock, University};
const NONE: TechRequirement = TechRequirement::Age(Age::Feudal);
const CASTLE: TechRequirement = TechRequirement::Age(Age::Castle);
const IMPERIAL: TechRequirement = TechRequirement::Age(Age::Imperial);
const fn after(tech: Tech) -> TechRequirement {
    TechRequirement::Technology(tech)
}

/// In `Tech` order, from `FishingLines`.
pub(super) const RULES: [TechRules; 13] = [
    rule(
        Tech::FishingLines,
        Dock,
        Age::Feudal,
        cost(50, 100, 0),
        20,
        NONE,
        TechEffect::Rule(RuleEffect::FishingYield),
    ),
    rule(
        Tech::Gillnets,
        Dock,
        Age::Castle,
        cost(100, 125, 0),
        30,
        after(Tech::FishingLines),
        TechEffect::Rule(RuleEffect::FishingYield),
    ),
    rule(
        Tech::Careening,
        University,
        Age::Castle,
        cost(100, 0, 200),
        50,
        CASTLE,
        TechEffect::Rule(RuleEffect::Armor(ArmorClass::Ships)),
    ),
    rule(
        Tech::DryDock,
        University,
        Age::Imperial,
        cost(200, 0, 400),
        60,
        after(Tech::Careening),
        TechEffect::Rule(RuleEffect::Armor(ArmorClass::Ships)),
    ),
    rule(
        Tech::Shipwright,
        University,
        Age::Imperial,
        cost(1000, 0, 300),
        60,
        IMPERIAL,
        TechEffect::Rule(RuleEffect::ShipProduction),
    ),
    rule(
        Tech::ClinkerConstruction,
        University,
        Age::Castle,
        cost(150, 100, 0),
        50,
        CASTLE,
        TechEffect::Rule(RuleEffect::ShipSpeed),
    ),
    rule(
        Tech::CarvelHull,
        University,
        Age::Imperial,
        cost(350, 200, 0),
        60,
        after(Tech::ClinkerConstruction),
        TechEffect::Rule(RuleEffect::ShipSpeed),
    ),
    rule(
        Tech::Siphons,
        University,
        Age::Castle,
        cost(100, 0, 175),
        45,
        CASTLE,
        TechEffect::Rule(RuleEffect::FireShipAttack),
    ),
    rule(
        Tech::Incendiaries,
        University,
        Age::Imperial,
        cost(200, 0, 325),
        50,
        after(Tech::Siphons),
        TechEffect::Rule(RuleEffect::DeathBlast),
    ),
    rule(
        Tech::HeavyWarships,
        Dock,
        Age::Imperial,
        cost(0, 400, 315),
        65,
        after(Tech::WarGalley),
        TechEffect::UpgradeShips(HEAVY),
    ),
    rule(
        Tech::DemolitionShip,
        Dock,
        Age::Castle,
        cost(0, 25, 75),
        20,
        CASTLE,
        TechEffect::UpgradeShips(DEMOLITION),
    ),
    rule(
        Tech::HeavyDemolitionShip,
        Dock,
        Age::Imperial,
        cost(0, 250, 300),
        50,
        after(Tech::DemolitionShip),
        TechEffect::UpgradeShips(HEAVY_DEMOLITION),
    ),
    rule(
        Tech::EliteCannonGalleon,
        Dock,
        Age::Imperial,
        cost(0, 525, 500),
        30,
        IMPERIAL,
        TechEffect::UpgradeShips(ELITE_CANNON),
    ),
];
