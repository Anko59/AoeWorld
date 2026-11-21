//! Gunpowder and the generic late unit-line upgrades (track A, 2026-11-14).
//! Costs and times: SiegeEngineers/aoe2techtree data.json for DE update 188329
//! (`Tech[47]` Chemistry, `unit_upgrades[474|330|542|588]`). Effects: the DE
//! help strings (Chemistry: string 28047) and the Age of Empires wiki.
use super::super::{BuildingKind, Stockpile};
use super::rule_effect::RuleEffect;
use super::table::*;
use crate::UnitKind as U;

const fn imperial(
    tech: Tech,
    building: BuildingKind,
    (food, wood, gold): (u32, u32, u32),
    seconds: u32,
    requires: TechRequirement,
    effect: TechEffect,
) -> TechRules {
    TechRules {
        age: Age::Imperial,
        tech,
        building,
        cost: Stockpile {
            food,
            wood,
            gold,
            stone: 0,
        },
        seconds,
        requires,
        effect,
    }
}

pub(super) const RULES: [TechRules; 5] = [
    // Foot archers, mounted archers, ranged warships and fortifications +1
    // attack; gunpowder units are excluded. Required by Hand Cannoneer and
    // Bombard Cannon.
    imperial(
        Tech::Chemistry,
        BuildingKind::University,
        (300, 0, 200),
        100,
        TechRequirement::Age(Age::Imperial),
        TechEffect::Rule(RuleEffect::ProjectileAttack),
    ),
    imperial(
        Tech::HeavyCavalryArcher,
        BuildingKind::ArcheryRange,
        (900, 0, 500),
        50,
        TechRequirement::Age(Age::Imperial),
        TechEffect::UpgradeUnit {
            from: U::CavalryArcher,
            to: U::HeavyCavalryArcher,
        },
    ),
    imperial(
        Tech::HeavyCamelRider,
        BuildingKind::Stable,
        (325, 0, 360),
        105,
        TechRequirement::Age(Age::Imperial),
        TechEffect::UpgradeUnit {
            from: U::CamelRider,
            to: U::HeavyCamelRider,
        },
    ),
    imperial(
        Tech::HeavyScorpion,
        BuildingKind::SiegeWorkshop,
        (800, 750, 0),
        50,
        TechRequirement::Age(Age::Imperial),
        TechEffect::UpgradeUnit {
            from: U::Scorpion,
            to: U::HeavyScorpion,
        },
    ),
    imperial(
        Tech::SiegeOnager,
        BuildingKind::SiegeWorkshop,
        (1450, 0, 1000),
        150,
        TechRequirement::Technology(Tech::Onager),
        TechEffect::UpgradeUnit {
            from: U::Onager,
            to: U::SiegeOnager,
        },
    ),
];
