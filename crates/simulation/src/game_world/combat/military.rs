//! Feudal class subset and researched combat stats. DE sources: docs/military.md.
use super::{BuildingAttack, DamageKind, UnitCombat, building_combat, unit_combat};
use crate::{BuildingKind, GameWorld, Tech, UnitKind};
use aoe_core::{EntityId, PlayerId};

/// The movement projection of the same research rules, without constructing
/// and adjusting unrelated attack/armour fields for every moving body.
pub(crate) fn apply_movement_speed(kind: UnitKind, base: Option<u32>, feudal: bool) -> Option<u32> {
    if kind == UnitKind::Scout && feudal {
        Some(155)
    } else {
        base
    }
}

pub fn researched_stats(kind: UnitKind, feudal: bool, researched: u64) -> UnitCombat {
    apply_research_stats(*unit_combat(kind), feudal, researched)
}

/// Apply effects to supplied base data, including the browser's runtime asset.
pub fn apply_research_stats(mut s: UnitCombat, feudal: bool, researched: u64) -> UnitCombat {
    let kind = s.kind;
    s.speed_hundredths = apply_movement_speed(kind, s.speed_hundredths, feudal);
    let has = |t: Tech| researched & t.bit() != 0;
    let infantry = matches!(
        kind,
        UnitKind::Militia
            | UnitKind::ManAtArms
            | UnitKind::Spearman
            | UnitKind::LongSwordsman
            | UnitKind::Pikeman
    );
    let archer = matches!(
        kind,
        UnitKind::Archer | UnitKind::Skirmisher | UnitKind::Crossbowman | UnitKind::EliteSkirmisher
    );
    let cavalry = matches!(
        kind,
        UnitKind::Scout | UnitKind::Knight | UnitKind::LightCavalry
    );
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
    if (archer || matches!(kind, UnitKind::Galley | UnitKind::WarGalley)) && has(Tech::BodkinArrow)
    {
        s.attack += 1;
        s.range += 1;
        s.los += 1;
    }
    if (archer || matches!(kind, UnitKind::Galley | UnitKind::WarGalley)) && has(Tech::Fletching) {
        s.attack += 1;
        s.range += 1;
        s.los += 1;
    }
    s
}

pub fn class_bonus(attacker: UnitKind, victim: UnitKind) -> u16 {
    let cavalry = matches!(
        victim,
        UnitKind::Scout | UnitKind::Knight | UnitKind::LightCavalry
    );
    let archer = matches!(
        victim,
        UnitKind::Archer | UnitKind::Crossbowman | UnitKind::Skirmisher | UnitKind::EliteSkirmisher
    );
    let spear = matches!(victim, UnitKind::Spearman | UnitKind::Pikeman);
    match attacker {
        UnitKind::Spearman if cavalry => 15,
        UnitKind::Pikeman if cavalry => 22,
        UnitKind::Skirmisher if archer || spear => 3,
        UnitKind::EliteSkirmisher if archer || spear => 4,
        UnitKind::Archer | UnitKind::Crossbowman if spear => 3,
        UnitKind::LightCavalry if victim == UnitKind::Monk => 10,
        UnitKind::BatteringRam if matches!(victim, UnitKind::Mangonel | UnitKind::BatteringRam) => {
            40
        }
        UnitKind::Mangonel if matches!(victim, UnitKind::Mangonel | UnitKind::BatteringRam) => 12,
        _ => 0,
    }
}

impl GameWorld {
    pub fn unit_stats(&self, id: EntityId) -> Option<UnitCombat> {
        let u = self.unit(id)?;
        Some(researched_stats(
            u.kind,
            self.is_feudal(u.player),
            self.player_economy(u.player).map_or(0, |p| p.researched),
        ))
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
        for tech in [Tech::Fletching, Tech::BodkinArrow] {
            if self.has_researched(player, tech) {
                s.attack += 1;
                if kind != BuildingKind::TownCenter {
                    s.range += 1;
                }
            }
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
