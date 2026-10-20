//! The single building data table: costs, times, hit points, sizes and roles.
//! Values and data provenance: docs/economy.md. Times are seconds for one builder.
use super::{BuildingKind, Stockpile, UnitKind};
use crate::Age;
use aoe_map::ResourceKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildingRules {
    pub kind: BuildingKind,
    /// Charged when the foundation is placed (per segment for palisades).
    pub cost: Stockpile,
    pub build_seconds: u32,
    pub max_hp: u16,
    /// Footprint width in tiles.
    pub side: i32,
    /// Footprint depth; only the fixed-orientation gate is rectangular.
    pub depth: i32,
    /// Authoritative circular line of sight in tiles.
    pub los: u8,
    /// Population housed once complete.
    pub population_support: u16,
    /// Resources a villager may deposit here, Food/Wood/Gold/Stone order.
    pub accepts: [bool; 4],
    /// Units its training queue accepts; the combat table owns Militia cost and time.
    pub trainable: &'static [UnitKind],
    /// A completed building of this kind of the same player must exist.
    pub requires: Option<BuildingKind>,
    /// Villagers can construct it (the Town Center is placed by the start).
    pub buildable: bool,
    /// Units cannot walk through it (farms are walked on).
    pub blocks: bool,
    /// Food a farm holds when complete.
    pub food: u16,
    pub age: Age,
}

const fn cost(food: u32, wood: u32, gold: u32, stone: u32) -> Stockpile {
    Stockpile {
        food,
        wood,
        gold,
        stone,
    }
}
const NONE: [bool; 4] = [false; 4];
const ALL: [bool; 4] = [true; 4];
const NO_UNITS: &[UnitKind] = &[];
const VILLAGERS: &[UnitKind] = &[UnitKind::Villager];
const MILITIA: &[UnitKind] = &[UnitKind::Militia];

/// Canonical order of `BuildingKind`.
pub const BUILDING_RULES: [BuildingRules; 16] = [
    BuildingRules {
        kind: BuildingKind::TownCenter,
        cost: cost(0, 275, 0, 100),
        build_seconds: 150,
        max_hp: 2400,
        side: 4,
        depth: 4,
        los: 8,
        population_support: 5,
        accepts: ALL,
        trainable: VILLAGERS,
        requires: None,
        buildable: false,
        blocks: true,
        food: 0,
        age: Age::Dark,
    },
    BuildingRules {
        kind: BuildingKind::House,
        cost: cost(0, 25, 0, 0),
        build_seconds: 25,
        max_hp: 550,
        side: 2,
        depth: 2,
        los: 2,
        population_support: 5,
        accepts: NONE,
        trainable: NO_UNITS,
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Dark,
    },
    BuildingRules {
        kind: BuildingKind::LumberCamp,
        cost: cost(0, 100, 0, 0),
        build_seconds: 35,
        max_hp: 600,
        side: 2,
        depth: 2,
        los: 4,
        population_support: 0,
        accepts: [false, true, false, false],
        trainable: NO_UNITS,
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Dark,
    },
    BuildingRules {
        kind: BuildingKind::MiningCamp,
        cost: cost(0, 100, 0, 0),
        build_seconds: 35,
        max_hp: 600,
        side: 2,
        depth: 2,
        los: 4,
        population_support: 0,
        accepts: [false, false, true, true],
        trainable: NO_UNITS,
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Dark,
    },
    BuildingRules {
        kind: BuildingKind::Mill,
        cost: cost(0, 100, 0, 0),
        build_seconds: 35,
        max_hp: 600,
        side: 2,
        depth: 2,
        los: 4,
        population_support: 0,
        accepts: [true, false, false, false],
        trainable: NO_UNITS,
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Dark,
    },
    BuildingRules {
        kind: BuildingKind::Farm,
        cost: cost(0, 60, 0, 0),
        build_seconds: 15,
        max_hp: 480,
        side: 3,
        depth: 3,
        los: 2,
        population_support: 0,
        accepts: NONE,
        trainable: NO_UNITS,
        requires: Some(BuildingKind::Mill),
        buildable: true,
        blocks: false,
        food: 175,
        age: Age::Dark,
    },
    BuildingRules {
        kind: BuildingKind::Barracks,
        cost: cost(0, 175, 0, 0),
        build_seconds: 50,
        max_hp: 1200,
        side: 3,
        depth: 3,
        los: 5,
        population_support: 0,
        accepts: NONE,
        trainable: MILITIA,
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Dark,
    },
    BuildingRules {
        kind: BuildingKind::PalisadeWall,
        cost: cost(0, 2, 0, 0),
        build_seconds: 6,
        max_hp: 250,
        side: 1,
        depth: 1,
        los: 2,
        population_support: 0,
        accepts: NONE,
        trainable: NO_UNITS,
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Dark,
    },
    BuildingRules {
        kind: BuildingKind::Outpost,
        cost: cost(0, 25, 0, 5),
        build_seconds: 15,
        max_hp: 500,
        side: 1,
        depth: 1,
        los: 10,
        population_support: 0,
        accepts: NONE,
        trainable: NO_UNITS,
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Dark,
    },
    BuildingRules {
        kind: BuildingKind::Dock,
        cost: cost(0, 150, 0, 0),
        build_seconds: 35,
        max_hp: 1800,
        side: 3,
        depth: 3,
        los: 8,
        population_support: 0,
        accepts: [true, false, false, false],
        trainable: &[UnitKind::FishingShip],
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Dark,
    },
    BuildingRules {
        kind: BuildingKind::ArcheryRange,
        cost: cost(0, 175, 0, 0),
        build_seconds: 50,
        max_hp: 1500,
        side: 3,
        depth: 3,
        los: 6,
        population_support: 0,
        accepts: NONE,
        trainable: NO_UNITS,
        requires: Some(BuildingKind::Barracks),
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Feudal,
    },
    BuildingRules {
        kind: BuildingKind::Stable,
        cost: cost(0, 175, 0, 0),
        build_seconds: 50,
        max_hp: 1500,
        side: 3,
        depth: 3,
        los: 6,
        population_support: 0,
        accepts: NONE,
        trainable: NO_UNITS,
        requires: Some(BuildingKind::Barracks),
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Feudal,
    },
    BuildingRules {
        kind: BuildingKind::Blacksmith,
        cost: cost(0, 150, 0, 0),
        build_seconds: 40,
        max_hp: 1800,
        side: 3,
        depth: 3,
        los: 6,
        population_support: 0,
        accepts: NONE,
        trainable: NO_UNITS,
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Feudal,
    },
    BuildingRules {
        kind: BuildingKind::Market,
        cost: cost(0, 175, 0, 0),
        build_seconds: 60,
        max_hp: 1800,
        side: 4,
        depth: 4,
        los: 6,
        population_support: 0,
        accepts: NONE,
        trainable: NO_UNITS,
        requires: Some(BuildingKind::Mill),
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Feudal,
    },
    BuildingRules {
        kind: BuildingKind::WatchTower,
        cost: cost(0, 35, 0, 125),
        build_seconds: 80,
        max_hp: 850,
        side: 1,
        depth: 1,
        los: 10,
        population_support: 0,
        accepts: NONE,
        trainable: NO_UNITS,
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Feudal,
    },
    BuildingRules {
        kind: BuildingKind::PalisadeGate,
        cost: cost(0, 30, 0, 0),
        build_seconds: 30,
        max_hp: 240,
        side: 4,
        depth: 1,
        los: 6,
        population_support: 0,
        accepts: NONE,
        trainable: NO_UNITS,
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Dark,
    },
];

/// The population ceiling regardless of houses.
pub const MAX_POPULATION: u16 = 200;
/// The AoE II rule: n builders take `base * 3 / (n + 2)`.
pub const BUILD_WORK_PER_BASE_TICK: u64 = 3;
/// Longest palisade a single order may lay down.
pub const MAX_WALL_SEGMENTS: usize = 64;

impl BuildingKind {
    /// DE's upgraded unit records, plus the automatic palisade HP effect 293.
    pub fn age_stats(self, age: Age) -> (u16, u8) {
        if age == Age::Feudal {
            use BuildingKind::*;
            match self {
                House => (750, 2),
                LumberCamp | MiningCamp | Mill => (800, 6),
                Barracks => (1500, 6),
                PalisadeGate => (400, 6),
                _ => (self.rules().max_hp, self.rules().los),
            }
        } else {
            (self.rules().max_hp, self.rules().los)
        }
    }
    pub fn rules(self) -> &'static BuildingRules {
        &BUILDING_RULES[self as usize]
    }
    /// Can a villager carrying `kind` deposit here?
    pub fn accepts(self, kind: ResourceKind) -> bool {
        self.rules().accepts[kind as usize]
    }
}

/// Units that a building of this kind can train.
pub fn trainable_units(kind: BuildingKind) -> &'static [UnitKind] {
    kind.rules().trainable
}
