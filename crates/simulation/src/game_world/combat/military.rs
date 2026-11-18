//! Feudal class subset and researched combat stats. DE sources: docs/military.md.
use super::{BuildingAttack, DamageKind, UnitCombat, building_combat, unit_combat};
use crate::{BuildingKind, GameWorld, Tech, UnitKind};
use aoe_core::{EntityId, PlayerId};

/// The movement projection of the same research rules, without constructing
/// and adjusting unrelated attack/armour fields for every moving body.
pub(crate) fn apply_movement_speed(
    kind: UnitKind,
    base: Option<u32>,
    feudal: bool,
    researched: u128,
) -> Option<u32> {
    let base = if kind == UnitKind::Scout && feudal {
        Some(155)
    } else {
        base
    };
    if researched == 0 {
        return base;
    }
    let percent = u32::from(crate::game_world::research::unit_speed_percent(
        kind, researched,
    ));
    base.map(|speed| speed * percent / 100)
}

/// Carvel Hull and Clinker Construction: ships move 10% faster each. Fishing
/// Ships keep their own speed.
pub(crate) fn naval_speed(kind: UnitKind, speed: u32, researched: u128) -> u32 {
    if kind == UnitKind::FishingShip {
        return speed;
    }
    let steps = u32::from(researched & Tech::ClinkerConstruction.bit() != 0)
        + u32::from(researched & Tech::CarvelHull.bit() != 0);
    speed * (100 + 10 * steps) / 100
}

pub fn researched_stats(kind: UnitKind, feudal: bool, researched: u128) -> UnitCombat {
    apply_research_stats(*unit_combat(kind), feudal, researched)
}

/// Foot and mounted archers: the archer armour line and the Fletching line.
fn is_missile(kind: UnitKind) -> bool {
    matches!(
        kind,
        UnitKind::Archer
            | UnitKind::Skirmisher
            | UnitKind::Crossbowman
            | UnitKind::EliteSkirmisher
            | UnitKind::Arbalester
            | UnitKind::CavalryArcher
            | UnitKind::HeavyCavalryArcher
    )
}

/// `researched_stats(..).los` alone: fog asks it for every body that moves.
pub(crate) fn researched_los(kind: UnitKind, feudal: bool, researched: u128) -> u8 {
    let has = |t: Tech| u8::from(researched & t.bit() != 0);
    if kind == UnitKind::Scout && feudal {
        return if has(Tech::CastleAge) == 1 { 8 } else { 6 };
    }
    let base = unit_combat(kind).los;
    if researched == 0 {
        return base;
    }
    let lines = has(Tech::BodkinArrow) + has(Tech::Fletching);
    if is_missile(kind) {
        base + has(Tech::Bracer) + lines
    } else if kind.is_galley_line() {
        base + lines
    } else {
        base
    }
}

/// Apply effects to supplied base data, including the browser's runtime asset.
pub fn apply_research_stats(mut s: UnitCombat, feudal: bool, researched: u128) -> UnitCombat {
    // Only scouts have an age effect without research.
    if researched == 0 && (!feudal || s.kind != UnitKind::Scout) {
        return s;
    }
    let kind = s.kind;
    s.speed_hundredths = apply_movement_speed(kind, s.speed_hundredths, feudal, researched);
    let has = |t: Tech| researched & t.bit() != 0;
    let infantry = matches!(
        kind,
        UnitKind::Militia
            | UnitKind::ManAtArms
            | UnitKind::Spearman
            | UnitKind::LongSwordsman
            | UnitKind::Pikeman
            | UnitKind::TwoHandedSwordsman
            | UnitKind::Champion
            | UnitKind::Halberdier
    );
    // Foot archers and mounted archers take the archer armour line; the
    // Hand Cannoneer takes only the armour (no Fletching line, no Chemistry).
    let missile = is_missile(kind);
    let archer = missile || kind == UnitKind::HandCannoneer;
    let ranged_ship = kind.is_galley_line();
    let cavalry = matches!(
        kind,
        UnitKind::Scout
            | UnitKind::Knight
            | UnitKind::LightCavalry
            | UnitKind::Cavalier
            | UnitKind::Paladin
            | UnitKind::Hussar
            | UnitKind::CamelRider
            | UnitKind::HeavyCamelRider
    );
    if kind.is_naval() {
        // Careening and Dry Dock: +1 pierce armour each for every ship.
        s.pierce_armour += u16::from(has(Tech::Careening)) + u16::from(has(Tech::DryDock));
        s.speed_hundredths = s
            .speed_hundredths
            .map(|speed| naval_speed(kind, speed, researched));
        if kind.is_hulk_line() {
            // Forging +1, Iron Casting +1, Blast Furnace +2 for the Hulk line
            // (help strings 28067, 28068 and 28075).
            s.attack += u16::from(has(Tech::Forging))
                + u16::from(has(Tech::IronCasting))
                + 2 * u16::from(has(Tech::BlastFurnace));
        }
        if kind.is_fire_line() && has(Tech::Siphons) {
            s.attack += 1;
        }
    }
    if kind == UnitKind::Scout && feudal {
        s.attack += 2;
        s.los = if has(Tech::CastleAge) { 8 } else { 6 };
    }
    if has(Tech::Forging) && (infantry || cavalry) {
        s.attack += 1;
    }
    if (infantry && has(Tech::ScaleMailArmor))
        || (cavalry && has(Tech::ScaleBardingArmor))
        || (archer && has(Tech::PaddedArcherArmor))
    {
        s.melee_armour += 1;
        s.pierce_armour += 1;
    }
    if has(Tech::IronCasting) && (infantry || cavalry) {
        s.attack += 2;
    }
    if infantry && has(Tech::ChainMailArmor)
        || cavalry && has(Tech::ChainBardingArmor)
        || archer && has(Tech::LeatherArcherArmor)
    {
        s.melee_armour += 1;
        s.pierce_armour += 1;
    }
    if has(Tech::BlastFurnace) && (infantry || cavalry) {
        s.attack += 2;
    }
    if infantry && has(Tech::PlateMailArmor)
        || cavalry && has(Tech::PlateBardingArmor)
        || archer && has(Tech::RingArcherArmor)
    {
        s.melee_armour += 1;
        s.pierce_armour += 2;
    }
    // Gambesons: militia line +1 pierce armour (Fire Lancers and Varangian
    // Guards join with their units).
    if kind.is_militia_line() && has(Tech::Gambesons) {
        s.pierce_armour += 1;
    }
    // Thumb Ring: Archery Units and Skirmishers attack 15% faster. Accuracy
    // is already 100%: projectiles have no accuracy dice (projectiles.rs).
    if (kind.is_archery() || kind.is_mounted_archer()) && has(Tech::ThumbRing) {
        s.attack_period_centiseconds = s.attack_period_centiseconds * 100 / 115;
    }
    // Parthian Tactics: Mounted Archers +1 melee / +2 pierce armour.
    if kind.is_mounted_archer() && has(Tech::ParthianTactics) {
        s.melee_armour += 1;
        s.pierce_armour += 2;
    }
    // Siege Engineers: artillery +1 range.
    if kind.is_artillery() && has(Tech::SiegeEngineers) {
        s.range += 1;
    }
    // Block Printing: Monks convert from 3 tiles farther.
    if kind == UnitKind::Monk && has(Tech::BlockPrinting) {
        s.range += 3;
    }
    if (missile || ranged_ship) && has(Tech::Chemistry) {
        s.attack += 1;
    }
    if missile && has(Tech::Bracer) {
        s.attack += 1;
        s.range += 1;
        s.los += 1;
    }
    if (missile || ranged_ship) && has(Tech::BodkinArrow) {
        s.attack += 1;
        s.range += 1;
        s.los += 1;
    }
    if (missile || ranged_ship) && has(Tech::Fletching) {
        s.attack += 1;
        s.range += 1;
        s.los += 1;
    }
    s
}

pub fn class_bonus(attacker: UnitKind, victim: UnitKind) -> u16 {
    super::classes::unit_bonus(attacker, victim)
}

impl GameWorld {
    pub fn unit_stats(&self, id: EntityId) -> Option<UnitCombat> {
        self.unit(id).map(|unit| self.unit_state_stats(&unit))
    }
    pub(super) fn unit_state_stats(&self, unit: &crate::GameUnit) -> UnitCombat {
        let economy = self.economy.players.get(&unit.player);
        researched_stats(
            unit.kind,
            economy.is_some_and(|p| p.age >= crate::Age::Feudal),
            economy.map_or(0, |p| p.researched),
        )
    }

    pub fn building_attack(&self, player: PlayerId, kind: BuildingKind) -> Option<BuildingAttack> {
        let mut s = if kind == BuildingKind::TownCenter {
            // The firepower calculation supplies zero arrows without a garrison.
            BuildingAttack {
                attack: 5,
                kind: DamageKind::Pierce,
                min_range: 0,
                range: 6,
                period_centiseconds: 200,
            }
        } else {
            building_combat(kind).attack?
        };
        let researched = self.player_economy(player).map_or(0, |p| p.researched);
        s.attack = crate::game_world::research::tower_attack(kind, s.attack, researched);
        for tech in [Tech::Fletching, Tech::BodkinArrow, Tech::Bracer] {
            if self.has_researched(player, tech) {
                s.attack += 1;
                if kind != BuildingKind::TownCenter {
                    s.range += 1;
                }
            }
        }
        // Chemistry: ranged fortifications +1 attack, no range.
        if self.has_researched(player, Tech::Chemistry) {
            s.attack += 1;
        }
        if self.has_researched(player, Tech::MurderHoles) && kind != BuildingKind::TownCenter {
            s.min_range = 0;
        }
        Some(s)
    }
    pub(crate) fn target_visible_to(&self, player: PlayerId, target: EntityId) -> bool {
        if let Some(u) = self.unit(target) {
            return self.unit_visible_to(player, u.id);
        }
        if let Some(a) = self.animal(target) {
            return self.visibility(player, a.position.tile_floor())
                == crate::TileVisibility::Visible;
        }
        self.building(target).is_some_and(|b| {
            (b.footprint.min.y..b.footprint.max.y).any(|y| {
                (b.footprint.min.x..b.footprint.max.x).any(|x| {
                    self.visibility(player, aoe_core::TileCoord::new(x, y))
                        == crate::TileVisibility::Visible
                })
            })
        })
    }
}

#[path = "military/tests.rs"]
#[cfg(test)]
mod tests;
