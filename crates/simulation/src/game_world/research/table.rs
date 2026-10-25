//! The single technology data table, in the manner of `BUILDING_RULES`. Fix a
//! number or a requirement here and the rules, the tests and the wire follow.
//! Values and data provenance: docs/economy.md.
use super::super::{BuildingKind, Stockpile};

/// The player's age, used by building and technology requirements.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Age {
    #[default]
    Dark,
    Feudal,
    Castle,
}

/// Canonical order; a technology's bit in `researched` is `1 << tech as u8`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Tech {
    Loom,
    TownWatch,
    FeudalAge,
    DoubleBitAxe,
    HorseCollar,
    GoldMining,
    StoneMining,
    Wheelbarrow,
    ManAtArms,
    Forging,
    ScaleMailArmor,
    ScaleBardingArmor,
    Fletching,
    PaddedArcherArmor,
    CastleAge,
    LongSwordsman,
    Pikeman,
    Crossbowman,
    EliteSkirmisher,
    LightCavalry,
    HandCart,
    TownPatrol,
    IronCasting,
    ChainMailArmor,
    ChainBardingArmor,
    BodkinArrow,
    LeatherArcherArmor,
    HeavyPlow,
    BowSaw,
    GoldShaftMining,
    Masonry,
    Ballistics,
    MurderHoles,
    WarGalley,
}

/// What a finished technology changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TechEffect {
    Military,
    UpgradeMilitia,
    UpgradeUnit {
        from: super::super::UnitKind,
        to: super::super::UnitKind,
    },
    Masonry,
    UpgradeGalley,
    /// Every villager of the player, existing and future.
    VillagerBonus {
        hp: u16,
        melee_armour: i16,
        pierce_armour: u16,
    },
    /// Every building of the player, existing and future.
    BuildingLos(u8),
    AgeUp(Age),
    GatherRate {
        resource: aoe_map::ResourceKind,
        percent: u16,
    },
    FarmFood(u16),
    VillagerEconomy {
        capacity_permyriad: u32,
        speed_percent: u16,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TechRequirement {
    None,
    Age(Age),
    Technology(Tech),
    /// At least `count` distinct kinds from `kinds`, each completed at least once.
    BuildingKinds {
        count: usize,
        kinds: &'static [BuildingKind],
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TechRules {
    pub age: Age,
    pub tech: Tech,
    /// The kind of building whose queue researches it.
    pub building: BuildingKind,
    pub cost: Stockpile,
    pub seconds: u32,
    pub requires: TechRequirement,
    pub effect: TechEffect,
}

const fn cost(food: u32, wood: u32, gold: u32, stone: u32) -> Stockpile {
    Stockpile {
        food,
        wood,
        gold,
        stone,
    }
}

/// Dark Age buildings that count towards the Feudal Age. Houses, walls and
/// outposts do not. Docks count alongside the other economic buildings.
pub const FEUDAL_BUILDINGS: &[BuildingKind] = &[
    BuildingKind::Dock,
    BuildingKind::Barracks,
    BuildingKind::Mill,
    BuildingKind::LumberCamp,
    BuildingKind::MiningCamp,
];

/// Indexed by `Tech as usize`; a test pins the order.
pub const TECH_RULES: [TechRules; 34] = [
    TechRules {
        age: Age::Dark,
        tech: Tech::Loom,
        building: BuildingKind::TownCenter,
        cost: cost(0, 0, 50, 0),
        seconds: 25,
        requires: TechRequirement::None,
        effect: TechEffect::VillagerBonus {
            hp: 15,
            melee_armour: 1,
            pierce_armour: 2,
        },
    },
    TechRules {
        age: Age::Feudal,
        tech: Tech::TownWatch,
        building: BuildingKind::TownCenter,
        cost: cost(75, 0, 0, 0),
        seconds: 25,
        requires: TechRequirement::Age(Age::Feudal),
        effect: TechEffect::BuildingLos(4),
    },
    TechRules {
        age: Age::Dark,
        tech: Tech::FeudalAge,
        building: BuildingKind::TownCenter,
        cost: cost(500, 0, 0, 0),
        seconds: 130,
        requires: TechRequirement::BuildingKinds {
            count: 2,
            kinds: FEUDAL_BUILDINGS,
        },
        effect: TechEffect::AgeUp(Age::Feudal),
    },
    TechRules {
        age: Age::Feudal,
        tech: Tech::DoubleBitAxe,
        building: BuildingKind::LumberCamp,
        cost: cost(100, 50, 0, 0),
        seconds: 25,
        requires: TechRequirement::Age(Age::Feudal),
        effect: TechEffect::GatherRate {
            resource: aoe_map::ResourceKind::Wood,
            percent: 120,
        },
    },
    TechRules {
        age: Age::Feudal,
        tech: Tech::HorseCollar,
        building: BuildingKind::Mill,
        cost: cost(75, 75, 0, 0),
        seconds: 20,
        requires: TechRequirement::Age(Age::Feudal),
        effect: TechEffect::FarmFood(75),
    },
    TechRules {
        age: Age::Feudal,
        tech: Tech::GoldMining,
        building: BuildingKind::MiningCamp,
        cost: cost(100, 75, 0, 0),
        seconds: 30,
        requires: TechRequirement::Age(Age::Feudal),
        effect: TechEffect::GatherRate {
            resource: aoe_map::ResourceKind::Gold,
            percent: 115,
        },
    },
    TechRules {
        age: Age::Feudal,
        tech: Tech::StoneMining,
        building: BuildingKind::MiningCamp,
        cost: cost(100, 75, 0, 0),
        seconds: 30,
        requires: TechRequirement::Age(Age::Feudal),
        effect: TechEffect::GatherRate {
            resource: aoe_map::ResourceKind::Stone,
            percent: 115,
        },
    },
    TechRules {
        age: Age::Feudal,
        tech: Tech::Wheelbarrow,
        building: BuildingKind::TownCenter,
        cost: cost(175, 50, 0, 0),
        seconds: 75,
        requires: TechRequirement::Age(Age::Feudal),
        effect: TechEffect::VillagerEconomy {
            capacity_permyriad: 12695,
            speed_percent: 110,
        },
    },
    TechRules {
        age: Age::Feudal,
        tech: Tech::ManAtArms,
        building: BuildingKind::Barracks,
        cost: cost(100, 0, 40, 0),
        seconds: 40,
        requires: TechRequirement::Age(Age::Feudal),
        effect: TechEffect::UpgradeMilitia,
    },
    military(Tech::Forging, 150, 0, 50),
    military(Tech::ScaleMailArmor, 100, 0, 40),
    military(Tech::ScaleBardingArmor, 150, 0, 45),
    military(Tech::Fletching, 100, 50, 30),
    military(Tech::PaddedArcherArmor, 100, 0, 40),
    super::castle::RULES[0],
    super::castle::RULES[1],
    super::castle::RULES[2],
    super::castle::RULES[3],
    super::castle::RULES[4],
    super::castle::RULES[5],
    super::castle::RULES[6],
    super::castle::RULES[7],
    super::castle::RULES[8],
    super::castle::RULES[9],
    super::castle::RULES[10],
    super::castle::RULES[11],
    super::castle::RULES[12],
    super::castle::RULES[13],
    super::castle::RULES[14],
    super::castle::RULES[15],
    super::castle::RULES[16],
    super::castle::RULES[17],
    super::castle::RULES[18],
    TechRules {
        age: Age::Castle,
        tech: Tech::WarGalley,
        building: BuildingKind::Dock,
        cost: cost(230, 0, 100, 0),
        seconds: 50,
        requires: TechRequirement::Age(Age::Castle),
        effect: TechEffect::UpgradeGalley,
    },
];

const fn military(tech: Tech, food: u32, gold: u32, seconds: u32) -> TechRules {
    TechRules {
        age: Age::Feudal,
        tech,
        building: BuildingKind::Blacksmith,
        cost: cost(food, 0, gold, 0),
        seconds,
        requires: TechRequirement::Age(Age::Feudal),
        effect: TechEffect::Military,
    }
}

impl Tech {
    pub const fn rules(self) -> &'static TechRules {
        &TECH_RULES[self as usize]
    }
    pub const fn bit(self) -> u64 {
        1 << self as u8
    }
}
