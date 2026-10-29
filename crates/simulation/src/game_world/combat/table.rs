//! The single combat data table. Dark Age unit rules retain their original
//! source; Feudal building armour and Watch Tower attacks are verified against
//! local DE VER 8.9. See docs/economy/feudal.md for the extraction and ids.
use super::super::{BuildingKind, UnitKind};

/// Which armour class reduces a hit: melee units or Watch Tower arrows.
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
    /// Walking speed in hundredths of a tile per second. `None` keeps the speed
    /// owned by `VILLAGE_RULES` (villager) or fishing rules.
    pub speed_hundredths: Option<u32>,
    pub range: u8,
    pub min_range: u8,
    pub los: u8,
    /// `None` for units not trained through this table (the villager uses
    /// `VILLAGE_RULES`). Starting and Stable scouts share this table.
    pub training: Option<UnitTraining>,
}

#[path = "table/units.rs"]
mod units;
pub use units::UNIT_COMBAT;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildingAttack {
    pub attack: u16,
    pub kind: DamageKind,
    pub min_range: u8,
    pub range: u8,
    pub period_centiseconds: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildingCombat {
    pub kind: BuildingKind,
    pub melee_armour: i16,
    pub pierce_armour: u16,
    pub attack: Option<BuildingAttack>,
}

const fn armour(kind: BuildingKind, melee_armour: i16, pierce_armour: u16) -> BuildingCombat {
    BuildingCombat {
        kind,
        melee_armour,
        pierce_armour,
        attack: None,
    }
}

/// Indexed by `BuildingKind as usize`. `BUILDING_RULES` owns hit points.
/// AoE II DE Dark Age armour, from the extracted game data:
/// https://github.com/SiegeEngineers/aoe2techtree/blob/master/data/data.json
/// House -2 melee: official AoE II DE update 56005:
/// https://www.ageofempires.com/news/aoeii_de_update_56005/
/// Farm and Outpost really have 0/0; these are not placeholders.
pub const BUILDING_COMBAT: [BuildingCombat; 21] = [
    armour(BuildingKind::TownCenter, 3, 5),
    armour(BuildingKind::House, -2, 7),
    armour(BuildingKind::LumberCamp, 0, 7),
    armour(BuildingKind::MiningCamp, 0, 7),
    armour(BuildingKind::Mill, 0, 7),
    armour(BuildingKind::Farm, 0, 0),
    armour(BuildingKind::Barracks, 0, 7),
    armour(BuildingKind::PalisadeWall, 2, 5),
    armour(BuildingKind::Outpost, 0, 0),
    armour(BuildingKind::Dock, 0, 7),
    armour(BuildingKind::ArcheryRange, 1, 8),
    armour(BuildingKind::Stable, 1, 8),
    armour(BuildingKind::Blacksmith, 1, 8),
    armour(BuildingKind::Market, 1, 8),
    BuildingCombat {
        attack: Some(BuildingAttack {
            attack: 5,
            kind: DamageKind::Pierce,
            min_range: 1,
            range: 8,
            period_centiseconds: 200,
        }),
        ..armour(BuildingKind::WatchTower, 1, 7)
    },
    armour(BuildingKind::PalisadeGate, 0, 0),
    BuildingCombat {
        attack: Some(BuildingAttack {
            attack: 11,
            kind: DamageKind::Pierce,
            min_range: 1,
            range: 8,
            period_centiseconds: 200,
        }),
        ..armour(BuildingKind::Castle, 8, 11)
    },
    armour(BuildingKind::Monastery, 2, 9),
    armour(BuildingKind::University, 2, 9),
    armour(BuildingKind::SiegeWorkshop, 1, 8),
    armour(BuildingKind::Bridge, 2, 5),
];

pub(crate) fn age_armour(kind: BuildingKind, age: crate::Age) -> BuildingCombat {
    let mut stats = *building_combat(kind);
    if age >= crate::Age::Feudal
        && matches!(
            kind,
            BuildingKind::TownCenter
                | BuildingKind::House
                | BuildingKind::LumberCamp
                | BuildingKind::MiningCamp
                | BuildingKind::Mill
                | BuildingKind::Barracks
        )
    {
        let bonus = if age == crate::Age::Castle { 2 } else { 1 };
        stats.melee_armour += bonus;
        stats.pierce_armour += bonus as u16;
    }
    if age == crate::Age::Castle
        && matches!(
            kind,
            BuildingKind::ArcheryRange
                | BuildingKind::Stable
                | BuildingKind::Blacksmith
                | BuildingKind::Market
                | BuildingKind::Dock
        )
    {
        stats.melee_armour += 1;
        stats.pierce_armour += 1;
    }
    stats
}

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
    damage_with_bonus(attack, kind, melee_armour, pierce_armour, 0)
}

pub(super) const fn damage_with_bonus(
    attack: u16,
    kind: DamageKind,
    melee_armour: i16,
    pierce_armour: u16,
    bonus: u16,
) -> u16 {
    let damage = damage_components(attack, kind, melee_armour, pierce_armour, bonus);
    if damage < 1 { 1 } else { damage }
}

pub(super) const fn damage_components(
    attack: u16,
    kind: DamageKind,
    melee_armour: i16,
    pierce_armour: u16,
    bonus: u16,
) -> u16 {
    let armour = match kind {
        DamageKind::Melee => melee_armour as i32,
        DamageKind::Pierce => pierce_armour as i32,
    };
    let base = attack as i32 - armour;
    let damage = (if base > 0 { base } else { 0 }) + bonus as i32;
    if damage > u16::MAX as i32 {
        u16::MAX
    } else {
        damage as u16
    }
}
