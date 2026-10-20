//! The single combat data table. Every Dark Age combat number lives here so a
//! wrong value is corrected in one place. Values are Age of Empires II:
//! Definitive Edition as recalled by the owner's brief, not verified in game.
use super::super::{BuildingKind, UnitKind};

/// Which armour class reduces a hit. Only melee is used by the current units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DamageKind {
    Melee,
    Pierce,
}

/// Resources paid when a unit is queued. Refunded on cancellation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UnitCost {
    pub food: u32,
    pub wood: u32,
    pub gold: u32,
    pub stone: u32,
}

/// Cost and build time for a unit that a production building can queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitTraining {
    pub cost: UnitCost,
    pub seconds: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitCombat {
    pub kind: UnitKind,
    pub hp: u16,
    pub attack: u16,
    pub attack_kind: DamageKind,
    pub melee_armour: i16,
    pub pierce_armour: u16,
    /// Time between hits, in hundredths of a second.
    pub attack_period_centiseconds: u32,
    /// Walking speed in tenths of a tile per second. `None` keeps the speed
    /// owned by `VILLAGE_RULES` (villager) or the world config (scout).
    pub speed_tenths: Option<u32>,
    /// `None` for units not trained through this table (the villager uses
    /// `VILLAGE_RULES`; the scout is a starting unit).
    pub training: Option<UnitTraining>,
}

/// Indexed by `UnitKind as usize`; a test pins the order.
pub const UNIT_COMBAT: [UnitCombat; 3] = [
    UnitCombat {
        kind: UnitKind::Villager,
        hp: 25,
        attack: 3,
        attack_kind: DamageKind::Melee,
        melee_armour: 0,
        pierce_armour: 0,
        attack_period_centiseconds: 200,
        speed_tenths: None,
        training: None,
    },
    UnitCombat {
        kind: UnitKind::Scout,
        hp: 45,
        attack: 3,
        attack_kind: DamageKind::Melee,
        melee_armour: 0,
        pierce_armour: 2,
        attack_period_centiseconds: 200,
        speed_tenths: None,
        training: None,
    },
    UnitCombat {
        kind: UnitKind::Militia,
        hp: 40,
        attack: 4,
        attack_kind: DamageKind::Melee,
        melee_armour: 0,
        pierce_armour: 1,
        attack_period_centiseconds: 200,
        speed_tenths: Some(9),
        training: Some(UnitTraining {
            cost: UnitCost {
                food: 60,
                wood: 0,
                gold: 20,
                stone: 0,
            },
            seconds: 21,
        }),
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildingCombat {
    pub kind: BuildingKind,
    pub melee_armour: i16,
    pub pierce_armour: u16,
}

const fn armour(kind: BuildingKind, melee_armour: i16, pierce_armour: u16) -> BuildingCombat {
    BuildingCombat {
        kind,
        melee_armour,
        pierce_armour,
    }
}

/// Indexed by `BuildingKind as usize`. `BUILDING_RULES` owns hit points.
/// AoE II DE Dark Age armour, from the extracted game data:
/// https://github.com/SiegeEngineers/aoe2techtree/blob/master/data/data.json
/// House -2 melee: official AoE II DE update 56005:
/// https://www.ageofempires.com/news/aoeii_de_update_56005/
/// Farm and Outpost really have 0/0; these are not placeholders.
pub const BUILDING_COMBAT: [BuildingCombat; 9] = [
    armour(BuildingKind::TownCenter, 3, 5),
    armour(BuildingKind::House, -2, 7),
    armour(BuildingKind::LumberCamp, 0, 7),
    armour(BuildingKind::MiningCamp, 0, 7),
    armour(BuildingKind::Mill, 0, 7),
    armour(BuildingKind::Farm, 0, 0),
    armour(BuildingKind::Barracks, 0, 7),
    armour(BuildingKind::PalisadeWall, 2, 5),
    armour(BuildingKind::Outpost, 0, 0),
];

pub const fn unit_combat(kind: UnitKind) -> &'static UnitCombat {
    &UNIT_COMBAT[kind as usize]
}
pub const fn building_combat(kind: BuildingKind) -> &'static BuildingCombat {
    &BUILDING_COMBAT[kind as usize]
}

/// Age of Empires II damage: the attack minus the matching armour, at least 1.
pub const fn damage_dealt(
    attack: u16,
    kind: DamageKind,
    melee_armour: i16,
    pierce_armour: u16,
) -> u16 {
    let armour = match kind {
        DamageKind::Melee => melee_armour as i32,
        DamageKind::Pierce => pierce_armour as i32,
    };
    let damage = attack as i32 - armour;
    if damage < 1 {
        1
    } else if damage > u16::MAX as i32 {
        u16::MAX
    } else {
        damage as u16
    }
}
