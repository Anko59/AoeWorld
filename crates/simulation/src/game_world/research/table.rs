//! The single technology data table, in the manner of `BUILDING_RULES`. Fix a
//! number or a requirement here and the rules, the tests and the wire follow.
//! Values are Age of Empires II: Definitive Edition's Dark Age.
use super::super::{BuildingKind, Stockpile};

/// The player's age. The Feudal Age unlocks nothing beyond this field yet.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum Age {
    #[default]
    Dark,
    Feudal,
}

/// Canonical order; a technology's bit in `researched` is `1 << tech as u8`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Tech {
    Loom,
    TownWatch,
    FeudalAge,
}

/// What a finished technology changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TechEffect {
    /// Every villager of the player, existing and future.
    VillagerBonus {
        hp: u16,
        melee_armour: i16,
        pierce_armour: u16,
    },
    /// Every building of the player, existing and future.
    BuildingLos(u8),
    AgeUp(Age),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TechRequirement {
    None,
    /// At least `count` distinct kinds from `kinds`, each completed at least once.
    BuildingKinds {
        count: usize,
        kinds: &'static [BuildingKind],
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TechRules {
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
/// outposts do not. There is no Dock building kind yet, so none is listed.
pub const FEUDAL_BUILDINGS: &[BuildingKind] = &[
    BuildingKind::Barracks,
    BuildingKind::Mill,
    BuildingKind::LumberCamp,
    BuildingKind::MiningCamp,
];

/// Indexed by `Tech as usize`; a test pins the order.
pub const TECH_RULES: [TechRules; 3] = [
    TechRules {
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
        tech: Tech::TownWatch,
        building: BuildingKind::TownCenter,
        cost: cost(75, 0, 0, 0),
        seconds: 25,
        requires: TechRequirement::None,
        effect: TechEffect::BuildingLos(4),
    },
    TechRules {
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
];

impl Tech {
    pub const fn rules(self) -> &'static TechRules {
        &TECH_RULES[self as usize]
    }
    pub const fn bit(self) -> u32 {
        1 << self as u8
    }
}
