//! Castle rules surveyed from the licensed DE DAT; see docs/economy/castle.md.
use super::super::{BuildingKind, Stockpile};
use super::table::*;
const fn cost(food: u32, wood: u32, gold: u32, stone: u32) -> Stockpile {
    Stockpile {
        food,
        wood,
        gold,
        stone,
    }
}
pub(super) const RULES: [TechRules; 19] = [
    TechRules {
        age: Age::Feudal,
        tech: Tech::CastleAge,
        building: BuildingKind::TownCenter,
        cost: cost(800, 0, 200, 0),
        seconds: 160,
        requires: TechRequirement::BuildingKinds {
            count: 2,
            kinds: CASTLE_BUILDINGS,
        },
        effect: TechEffect::AgeUp(Age::Castle),
    },
    castle(
        Tech::LongSwordsman,
        BuildingKind::Barracks,
        cost(150, 0, 65, 0),
        40,
        TechRequirement::Technology(Tech::ManAtArms),
        TechEffect::UpgradeUnit {
            from: crate::UnitKind::ManAtArms,
            to: crate::UnitKind::LongSwordsman,
        },
    ),
    castle(
        Tech::Pikeman,
        BuildingKind::Barracks,
        cost(160, 0, 90, 0),
        35,
        TechRequirement::None,
        TechEffect::UpgradeUnit {
            from: crate::UnitKind::Spearman,
            to: crate::UnitKind::Pikeman,
        },
    ),
    castle(
        Tech::Crossbowman,
        BuildingKind::ArcheryRange,
        cost(175, 0, 100, 0),
        35,
        TechRequirement::None,
        TechEffect::UpgradeUnit {
            from: crate::UnitKind::Archer,
            to: crate::UnitKind::Crossbowman,
        },
    ),
    castle(
        Tech::EliteSkirmisher,
        BuildingKind::ArcheryRange,
        cost(0, 230, 130, 0),
        50,
        TechRequirement::None,
        TechEffect::UpgradeUnit {
            from: crate::UnitKind::Skirmisher,
            to: crate::UnitKind::EliteSkirmisher,
        },
    ),
    castle(
        Tech::LightCavalry,
        BuildingKind::Stable,
        cost(150, 0, 50, 0),
        45,
        TechRequirement::None,
        TechEffect::UpgradeUnit {
            from: crate::UnitKind::Scout,
            to: crate::UnitKind::LightCavalry,
        },
    ),
    castle(
        Tech::HandCart,
        BuildingKind::TownCenter,
        cost(300, 200, 0, 0),
        55,
        TechRequirement::Technology(Tech::Wheelbarrow),
        TechEffect::VillagerEconomy {
            capacity_permyriad: 15000,
            speed_percent: 110,
        },
    ),
    castle(
        Tech::TownPatrol,
        BuildingKind::TownCenter,
        cost(300, 0, 100, 0),
        40,
        TechRequirement::Technology(Tech::TownWatch),
        TechEffect::BuildingLos(4),
    ),
    castle(
        Tech::IronCasting,
        BuildingKind::Blacksmith,
        cost(220, 0, 120, 0),
        75,
        TechRequirement::Technology(Tech::Forging),
        TechEffect::Military,
    ),
    castle(
        Tech::ChainMailArmor,
        BuildingKind::Blacksmith,
        cost(200, 0, 100, 0),
        55,
        TechRequirement::Technology(Tech::ScaleMailArmor),
        TechEffect::Military,
    ),
    castle(
        Tech::ChainBardingArmor,
        BuildingKind::Blacksmith,
        cost(250, 0, 150, 0),
        60,
        TechRequirement::Technology(Tech::ScaleBardingArmor),
        TechEffect::Military,
    ),
    castle(
        Tech::BodkinArrow,
        BuildingKind::Blacksmith,
        cost(200, 0, 100, 0),
        35,
        TechRequirement::Technology(Tech::Fletching),
        TechEffect::Military,
    ),
    castle(
        Tech::LeatherArcherArmor,
        BuildingKind::Blacksmith,
        cost(150, 0, 150, 0),
        55,
        TechRequirement::Technology(Tech::PaddedArcherArmor),
        TechEffect::Military,
    ),
    castle(
        Tech::HeavyPlow,
        BuildingKind::Mill,
        cost(125, 125, 0, 0),
        40,
        TechRequirement::Technology(Tech::HorseCollar),
        TechEffect::FarmFood(125),
    ),
    castle(
        Tech::BowSaw,
        BuildingKind::LumberCamp,
        cost(150, 100, 0, 0),
        50,
        TechRequirement::Technology(Tech::DoubleBitAxe),
        TechEffect::GatherRate {
            resource: aoe_map::ResourceKind::Wood,
            percent: 120,
        },
    ),
    castle(
        Tech::GoldShaftMining,
        BuildingKind::MiningCamp,
        cost(175, 75, 0, 0),
        75,
        TechRequirement::Technology(Tech::GoldMining),
        TechEffect::GatherRate {
            resource: aoe_map::ResourceKind::Gold,
            percent: 115,
        },
    ),
    castle(
        Tech::Masonry,
        BuildingKind::University,
        cost(150, 175, 0, 0),
        50,
        TechRequirement::None,
        TechEffect::Masonry,
    ),
    castle(
        Tech::Ballistics,
        BuildingKind::University,
        cost(0, 300, 175, 0),
        60,
        TechRequirement::None,
        TechEffect::Military,
    ),
    castle(
        Tech::MurderHoles,
        BuildingKind::University,
        cost(200, 0, 0, 100),
        35,
        TechRequirement::None,
        TechEffect::Military,
    ),
];
/// Two distinct Feudal production/economy buildings, excluding towers and gates.
pub const CASTLE_BUILDINGS: &[BuildingKind] = &[
    BuildingKind::ArcheryRange,
    BuildingKind::Stable,
    BuildingKind::Blacksmith,
    BuildingKind::Market,
];

const fn castle(
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
        age: Age::Castle,
    }
}
