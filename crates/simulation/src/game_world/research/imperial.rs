//! Imperial Age rules surveyed from the licensed DE DAT; see docs/economy/imperial.md.
use super::super::{BuildingKind, Stockpile};
use super::table::*;
use crate::UnitKind as U;
const fn cost(food: u32, wood: u32, gold: u32, stone: u32) -> Stockpile {
    Stockpile {
        food,
        wood,
        gold,
        stone,
    }
}
/// Two distinct completed Castle Age buildings, excluding Towers and Docks.
pub const IMPERIAL_BUILDINGS: &[BuildingKind] = &[
    BuildingKind::Castle,
    BuildingKind::University,
    BuildingKind::Monastery,
    BuildingKind::SiegeWorkshop,
];
const fn upgrade(from: U, to: U) -> TechEffect {
    TechEffect::UpgradeUnit { from, to }
}
const fn imperial(
    tech: Tech,
    building: BuildingKind,
    cost: Stockpile,
    seconds: u32,
    requires: TechRequirement,
    effect: TechEffect,
) -> TechRules {
    TechRules {
        tech,
        building,
        cost,
        seconds,
        requires,
        effect,
        age: Age::Imperial,
    }
}
const fn after(tech: Tech) -> TechRequirement {
    TechRequirement::Technology(tech)
}
const WOOD: TechEffect = TechEffect::GatherRate {
    resource: aoe_map::ResourceKind::Wood,
    percent: 120,
};
const STONE: TechEffect = TechEffect::GatherRate {
    resource: aoe_map::ResourceKind::Stone,
    percent: 115,
};
pub(super) const RULES: [TechRules; 19] = [
    TechRules {
        age: Age::Castle,
        tech: Tech::ImperialAge,
        building: BuildingKind::TownCenter,
        cost: cost(1000, 0, 800, 0),
        seconds: 190,
        requires: TechRequirement::BuildingKinds {
            count: 2,
            kinds: IMPERIAL_BUILDINGS,
        },
        effect: TechEffect::AgeUp(Age::Imperial),
    },
    imperial(
        Tech::TwoHandedSwordsman,
        BuildingKind::Barracks,
        cost(200, 0, 100, 0),
        45,
        after(Tech::LongSwordsman),
        upgrade(U::LongSwordsman, U::TwoHandedSwordsman),
    ),
    imperial(
        Tech::Champion,
        BuildingKind::Barracks,
        cost(650, 0, 350, 0),
        70,
        after(Tech::TwoHandedSwordsman),
        upgrade(U::TwoHandedSwordsman, U::Champion),
    ),
    imperial(
        Tech::Halberdier,
        BuildingKind::Barracks,
        cost(300, 0, 600, 0),
        50,
        after(Tech::Pikeman),
        upgrade(U::Pikeman, U::Halberdier),
    ),
    imperial(
        Tech::Arbalester,
        BuildingKind::ArcheryRange,
        cost(450, 0, 350, 0),
        50,
        after(Tech::Crossbowman),
        upgrade(U::Crossbowman, U::Arbalester),
    ),
    imperial(
        Tech::Cavalier,
        BuildingKind::Stable,
        cost(300, 0, 300, 0),
        80,
        TechRequirement::None,
        upgrade(U::Knight, U::Cavalier),
    ),
    imperial(
        Tech::Paladin,
        BuildingKind::Stable,
        cost(1300, 0, 750, 0),
        170,
        after(Tech::Cavalier),
        upgrade(U::Cavalier, U::Paladin),
    ),
    imperial(
        Tech::Hussar,
        BuildingKind::Stable,
        cost(500, 0, 600, 0),
        50,
        after(Tech::LightCavalry),
        upgrade(U::LightCavalry, U::Hussar),
    ),
    imperial(
        Tech::CappedRam,
        BuildingKind::SiegeWorkshop,
        cost(300, 0, 0, 0),
        50,
        TechRequirement::None,
        upgrade(U::BatteringRam, U::CappedRam),
    ),
    imperial(
        Tech::SiegeRam,
        BuildingKind::SiegeWorkshop,
        cost(1000, 0, 0, 0),
        75,
        after(Tech::CappedRam),
        upgrade(U::CappedRam, U::SiegeRam),
    ),
    imperial(
        Tech::Onager,
        BuildingKind::SiegeWorkshop,
        cost(800, 0, 500, 0),
        75,
        TechRequirement::None,
        upgrade(U::Mangonel, U::Onager),
    ),
    imperial(
        Tech::BlastFurnace,
        BuildingKind::Blacksmith,
        cost(275, 0, 225, 0),
        100,
        after(Tech::IronCasting),
        TechEffect::Military,
    ),
    imperial(
        Tech::PlateMailArmor,
        BuildingKind::Blacksmith,
        cost(300, 0, 150, 0),
        70,
        after(Tech::ChainMailArmor),
        TechEffect::Military,
    ),
    imperial(
        Tech::PlateBardingArmor,
        BuildingKind::Blacksmith,
        cost(350, 0, 200, 0),
        75,
        after(Tech::ChainBardingArmor),
        TechEffect::Military,
    ),
    imperial(
        Tech::Bracer,
        BuildingKind::Blacksmith,
        cost(300, 0, 200, 0),
        40,
        after(Tech::BodkinArrow),
        TechEffect::Military,
    ),
    imperial(
        Tech::RingArcherArmor,
        BuildingKind::Blacksmith,
        cost(250, 0, 250, 0),
        70,
        after(Tech::LeatherArcherArmor),
        TechEffect::Military,
    ),
    imperial(
        Tech::CropRotation,
        BuildingKind::Mill,
        cost(250, 250, 0, 0),
        70,
        after(Tech::HeavyPlow),
        TechEffect::FarmFood(175),
    ),
    imperial(
        Tech::TwoManSaw,
        BuildingKind::LumberCamp,
        cost(300, 200, 0, 0),
        100,
        after(Tech::BowSaw),
        WOOD,
    ),
    imperial(
        Tech::StoneShaftMining,
        BuildingKind::MiningCamp,
        cost(175, 75, 0, 0),
        75,
        after(Tech::StoneMining),
        STONE,
    ),
];
