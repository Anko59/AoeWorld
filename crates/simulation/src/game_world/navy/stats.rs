//! Naval combat and training records, indexed in appended unit-kind order.
//! Licensed original VER 5.7 DAT SHA-256
//! a213ed092071e1abd234181158c198f4a83ded970e308bf312eef66d752c3f2e:
//! Galley 539, Fire Ship 529, Demolition Ship 527, Transport 545, War Galley 21.
use crate::{DamageKind, UnitCombat, UnitCost, UnitKind, UnitTraining};

pub(in crate::game_world) const NAVAL: [UnitCombat; 5] = [
    ship(
        UnitKind::Galley,
        120,
        6,
        DamageKind::Pierce,
        0,
        6,
        300,
        143,
        5,
        7,
        90,
        30,
        36,
    ),
    ship(
        UnitKind::FireShip,
        100,
        2,
        DamageKind::Pierce,
        0,
        6,
        25,
        135,
        2,
        5,
        75,
        45,
        36,
    ),
    ship(
        UnitKind::DemolitionShip,
        50,
        110,
        DamageKind::Melee,
        0,
        3,
        500,
        160,
        0,
        6,
        70,
        50,
        31,
    ),
    ship(
        UnitKind::TransportShip,
        100,
        0,
        DamageKind::Melee,
        4,
        8,
        0,
        145,
        0,
        5,
        125,
        0,
        46,
    ),
    ship(
        UnitKind::WarGalley,
        135,
        7,
        DamageKind::Pierce,
        0,
        6,
        300,
        143,
        6,
        8,
        90,
        30,
        36,
    ),
];

pub(super) struct NavalDamage {
    pub(super) range_hundredths: u16,
    pub(super) contact_hundredths: u16,
    pub(super) blast_hundredths: u16,
    pub(super) ship_attack: u16,
    pub(super) ship_armour: u16,
    pub(super) building_attack: u16,
    pub(super) melee_attack: u16,
}

// Class 16 is ships, 11 buildings, 4 melee, 3 pierce. Demo's zero
// weapon range meets the two half-tile collision hulls at one tile.
pub(super) const DAMAGE: [NavalDamage; 5] = [
    damage(500, 0, 0, 8, 0, 6, 0),
    damage(249, 0, 0, 3, 4, 2, 1),
    damage(0, 100, 250, 0, 3, 220, 0),
    damage(0, 0, 0, 0, 0, 0, 0),
    damage(600, 0, 0, 9, 0, 7, 0),
];

const fn damage(
    range: u16,
    contact: u16,
    blast: u16,
    ship: u16,
    armour: u16,
    building: u16,
    melee: u16,
) -> NavalDamage {
    NavalDamage {
        range_hundredths: range,
        contact_hundredths: contact,
        blast_hundredths: blast,
        ship_attack: ship,
        ship_armour: armour,
        building_attack: building,
        melee_attack: melee,
    }
}

pub(super) fn damage_stats(kind: UnitKind) -> Option<&'static NavalDamage> {
    (kind as usize)
        .checked_sub(UnitKind::Galley as usize)
        .and_then(|index| DAMAGE.get(index))
}

pub(in crate::game_world) fn attack_range(stats: &UnitCombat) -> i64 {
    let hundredths = damage_stats(stats.kind).map_or(u16::from(stats.range) * 100, |naval| {
        naval.range_hundredths
            + naval.contact_hundredths
            + u16::from(
                stats
                    .range
                    .saturating_sub(crate::unit_combat(stats.kind).range),
            ) * 100
    });
    i64::from(hundredths) * i64::from(aoe_core::FIXED_SUBUNITS_PER_TILE) / 100
}

#[allow(clippy::too_many_arguments)]
const fn ship(
    kind: UnitKind,
    hp: u16,
    attack: u16,
    attack_kind: DamageKind,
    melee_armour: i16,
    pierce_armour: u16,
    period: u32,
    speed: u32,
    range: u8,
    los: u8,
    wood: u32,
    gold: u32,
    seconds: u32,
) -> UnitCombat {
    UnitCombat {
        kind,
        hp,
        attack,
        attack_kind,
        melee_armour,
        pierce_armour,
        attack_period_centiseconds: period,
        speed_hundredths: Some(speed),
        range,
        min_range: 0,
        los,
        training: Some(UnitTraining {
            cost: UnitCost {
                food: 0,
                wood,
                gold,
                stone: 0,
            },
            seconds,
        }),
    }
}
