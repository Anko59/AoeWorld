//! The single building data table: costs, times, hit points, sizes and roles.
//! Values and data provenance: docs/economy.md. Times are seconds for one builder.
#[path = "buildings/age.rs"]
mod age;
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
    /// Has movement collision; it can occupy only part of the placement
    /// footprint. Farms have none.
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
const MILITIA: &[UnitKind] = &[
    UnitKind::Militia,
    UnitKind::Spearman,
    UnitKind::ManAtArms,
    UnitKind::LongSwordsman,
    UnitKind::Pikeman,
];

/// Canonical order of `BuildingKind`.
pub const BUILDING_RULES: [BuildingRules; 21] = [
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
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Castle,
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
        trainable: &[
            UnitKind::FishingShip,
            UnitKind::Galley,
            UnitKind::FireShip,
            UnitKind::DemolitionShip,
            UnitKind::TransportShip,
            UnitKind::WarGalley,
        ],
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
        trainable: &[
            UnitKind::Archer,
            UnitKind::Skirmisher,
            UnitKind::Crossbowman,
            UnitKind::EliteSkirmisher,
        ],
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
        trainable: &[UnitKind::Scout, UnitKind::Knight, UnitKind::LightCavalry],
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
    castle(
        BuildingKind::Castle,
        cost(0, 0, 0, 650),
        200,
        4800,
        (4, 11),
        &[UnitKind::Vassal, UnitKind::General],
        None,
    ),
    castle(
        BuildingKind::Monastery,
        cost(0, 175, 0, 0),
        40,
        2100,
        (3, 6),
        &[UnitKind::Monk],
        None,
    ),
    castle(
        BuildingKind::University,
        cost(0, 200, 0, 0),
        60,
        2100,
        (4, 6),
        &[],
        None,
    ),
    castle(
        BuildingKind::SiegeWorkshop,
        cost(0, 200, 0, 0),
        40,
        1500,
        (4, 6),
        &[UnitKind::Mangonel, UnitKind::BatteringRam],
        Some(BuildingKind::Blacksmith),
    ),
    BuildingRules {
        kind: BuildingKind::Bridge,
        cost: cost(0, 25, 0, 5),
        build_seconds: 8,
        max_hp: 1200,
        side: 1,
        depth: 1,
        los: 4,
        population_support: 0,
        accepts: NONE,
        trainable: NO_UNITS,
        requires: None,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Feudal,
    },
];

const fn castle(
    kind: BuildingKind,
    cost: Stockpile,
    build_seconds: u32,
    max_hp: u16,
    shape: (i32, u8),
    trainable: &'static [UnitKind],
    requires: Option<BuildingKind>,
) -> BuildingRules {
    let (side, los) = shape;
    BuildingRules {
        kind,
        cost,
        build_seconds,
        max_hp,
        side,
        depth: side,
        los,
        population_support: if matches!(kind, BuildingKind::Castle) {
            20
        } else {
            0
        },
        accepts: NONE,
        trainable,
        requires,
        buildable: true,
        blocks: true,
        food: 0,
        age: Age::Castle,
    }
}

/// The population ceiling regardless of houses.
pub const MAX_POPULATION: u16 = 1_000;
/// The AoE II rule: n builders take `base * 3 / (n + 2)`.
pub const BUILD_WORK_PER_BASE_TICK: u64 = 3;
/// Longest palisade a single order may lay down.
pub const MAX_WALL_SEGMENTS: usize = 64;

impl BuildingKind {
    /// Movement collision is separate from the placement/selection footprint.
    /// AoK DAT 109 is non-obstructing; its annex 618 has half extents (1,1)
    /// at native NE/SE (1,-1), the NW 2x2 in our world axes. Annexes 619/620
    /// are non-obstructing. Construction unit 621 blocks 4x4 until replaced.
    pub(crate) fn collision_rect(
        self,
        footprint: aoe_core::TileRect,
        complete: bool,
    ) -> Option<aoe_core::TileRect> {
        (self.rules().blocks && !(self == Self::Bridge && complete)).then(|| {
            if self == Self::TownCenter && complete {
                aoe_core::TileRect::from_xywh(footprint.min.x, footprint.min.y, 2, 2)
            } else {
                footprint
            }
        })
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
