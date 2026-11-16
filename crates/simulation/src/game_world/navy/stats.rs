//! Naval combat and training records.
//! Numbers: SiegeEngineers/aoe2techtree data.json (DE Update 188329, after the
//! Naval Warfare Overhaul, Update 169123); see docs/navy.md for the rules.
//! DAT ids: Galley 539, War Galley 21, Galleon 442, Fire Galley 1103, Fire Ship
//! 529, Fast Fire Ship 532, Demolition Raft 1104, Demolition Ship 527, Heavy
//! Demolition Ship 528, Cannon Galleon 420, Elite 691, Hulk 2626, War Hulk 2627,
//! Carrack 2628, Catapult Galleon 2633, Transport 545.
use crate::{DamageKind, UnitCombat, UnitCost, UnitKind, UnitTraining};

/// Every ship except the Fishing Ship (see `UNIT_COMBAT`), keyed by `kind`.
pub(in crate::game_world) const NAVAL: [UnitCombat; 16] = [
    // kind, hp, attack, damage, melee armour, pierce armour, period (cs), speed (hundredths),
    // range, min range, los, wood, gold, seconds
    ship(
        UnitKind::Galley,
        (110, 6, DamageKind::Pierce, 0, 3),
        (300, 136),
        (5, 0, 7),
        (90, 30, 45),
    ),
    ship(
        UnitKind::FireShip,
        (120, 3, DamageKind::Melee, 2, 4),
        (25, 147),
        (2, 0, 5),
        (75, 45, 27),
    ),
    ship(
        UnitKind::DemolitionShip,
        (50, 95, DamageKind::Melee, 1, 0),
        (500, 152),
        (0, 0, 6),
        (45, 80, 31),
    ),
    ship(
        UnitKind::TransportShip,
        (100, 0, DamageKind::Melee, 4, 8),
        (0, 145),
        (0, 0, 5),
        (125, 0, 46),
    ),
    ship(
        UnitKind::WarGalley,
        (125, 7, DamageKind::Pierce, 0, 4),
        (300, 136),
        (6, 0, 8),
        (90, 30, 27),
    ),
    ship(
        UnitKind::FireGalley,
        (110, 2, DamageKind::Melee, 1, 4),
        (25, 136),
        (2, 0, 5),
        (75, 45, 49),
    ),
    ship(
        UnitKind::FastFireShip,
        (140, 4, DamageKind::Melee, 3, 7),
        (25, 156),
        (2, 0, 6),
        (75, 45, 27),
    ),
    ship(
        UnitKind::DemolitionRaft,
        (40, 75, DamageKind::Melee, 0, 0),
        (500, 142),
        (0, 0, 6),
        (45, 80, 45),
    ),
    ship(
        UnitKind::HeavyDemolitionShip,
        (70, 120, DamageKind::Melee, 2, 0),
        (500, 152),
        (0, 0, 6),
        (45, 80, 31),
    ),
    ship(
        UnitKind::Galleon,
        (155, 9, DamageKind::Pierce, 0, 6),
        (300, 136),
        (7, 0, 9),
        (90, 30, 27),
    ),
    ship(
        UnitKind::CannonGalleon,
        (120, 50, DamageKind::Melee, 0, 5),
        (1000, 105),
        (13, 3, 15),
        (200, 150, 46),
    ),
    ship(
        UnitKind::EliteCannonGalleon,
        (150, 60, DamageKind::Melee, 0, 6),
        (1000, 105),
        (15, 3, 17),
        (200, 150, 46),
    ),
    ship(
        UnitKind::Hulk,
        (85, 4, DamageKind::Melee, 3, 1),
        (175, 142),
        (1, 0, 5),
        (75, 35, 42),
    ),
    ship(
        UnitKind::WarHulk,
        (110, 4, DamageKind::Melee, 4, 1),
        (150, 142),
        (1, 0, 5),
        (75, 35, 27),
    ),
    ship(
        UnitKind::Carrack,
        (135, 4, DamageKind::Melee, 6, 1),
        (150, 152),
        (1, 0, 6),
        (75, 35, 27),
    ),
    ship(
        UnitKind::CatapultGalleon,
        (140, 30, DamageKind::Melee, 0, 6),
        (600, 100),
        (12, 3, 14),
        (225, 100, 55),
    ),
];

/// Weapon geometry in hundredths of a tile.
pub(super) struct NavalDamage {
    pub(super) range_hundredths: u16,
    pub(super) contact_hundredths: u16,
    pub(super) blast_hundredths: u16,
}

const fn damage(range: u16, contact: u16, blast: u16) -> NavalDamage {
    NavalDamage {
        range_hundredths: range,
        contact_hundredths: contact,
        blast_hundredths: blast,
    }
}

// Combat classes belong to combat/classes; these are weapon geometry only.
// Demolition blast radii are the DAT BlastWidth (2.5 / 3 / 3.25 tiles).
const NONE: NavalDamage = damage(0, 0, 0);
pub(super) fn damage_stats(kind: UnitKind) -> Option<&'static NavalDamage> {
    const GALLEY: NavalDamage = damage(500, 0, 0);
    const WAR_GALLEY: NavalDamage = damage(600, 0, 0);
    const GALLEON: NavalDamage = damage(700, 0, 0);
    const FIRE: NavalDamage = damage(250, 0, 0);
    const RAFT: NavalDamage = damage(0, 100, 250);
    const DEMOLITION: NavalDamage = damage(0, 100, 300);
    const HEAVY: NavalDamage = damage(0, 100, 325);
    const HULK: NavalDamage = damage(0, 100, 0);
    const CANNON: NavalDamage = damage(1300, 0, 0);
    const ELITE: NavalDamage = damage(1500, 0, 0);
    const CATAPULT: NavalDamage = damage(1200, 0, 0);
    Some(match kind {
        UnitKind::Galley => &GALLEY,
        UnitKind::WarGalley => &WAR_GALLEY,
        UnitKind::Galleon => &GALLEON,
        UnitKind::FireGalley | UnitKind::FireShip | UnitKind::FastFireShip => &FIRE,
        UnitKind::DemolitionRaft => &RAFT,
        UnitKind::DemolitionShip => &DEMOLITION,
        UnitKind::HeavyDemolitionShip => &HEAVY,
        UnitKind::Hulk | UnitKind::WarHulk | UnitKind::Carrack => &HULK,
        UnitKind::CannonGalleon => &CANNON,
        UnitKind::EliteCannonGalleon => &ELITE,
        UnitKind::CatapultGalleon => &CATAPULT,
        UnitKind::TransportShip => &NONE,
        _ => return None,
    })
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

/// `(hp, attack, kind, melee armour, pierce armour)`, `(period, speed)`,
/// `(range, min range, los)` and `(wood, gold, seconds)`.
const fn ship(
    kind: UnitKind,
    (hp, attack, attack_kind, melee_armour, pierce_armour): (u16, u16, DamageKind, i16, u16),
    (period, speed): (u32, u32),
    (range, min_range, los): (u8, u8, u8),
    (wood, gold, seconds): (u32, u32, u32),
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
        min_range,
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
