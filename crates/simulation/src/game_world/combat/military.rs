//! Feudal class subset and researched combat stats. DE sources: docs/military.md.
use super::{BuildingAttack, DamageKind, UnitCombat, building_combat, unit_combat};
use crate::{BuildingKind, GameWorld, Tech, UnitKind};
use aoe_core::{EntityId, PlayerId};

pub fn researched_stats(kind: UnitKind, feudal: bool, researched: u32) -> UnitCombat {
    let mut s = *unit_combat(kind);
    let has = |t: Tech| researched & t.bit() != 0;
    let infantry = matches!(
        kind,
        UnitKind::Militia | UnitKind::ManAtArms | UnitKind::Spearman
    );
    let archer = matches!(kind, UnitKind::Archer | UnitKind::Skirmisher);
    if kind == UnitKind::Scout && feudal {
        s.attack += 2;
        s.los = 6;
        s.speed_hundredths = Some(155);
    }
    if has(Tech::Forging) && (infantry || kind == UnitKind::Scout) {
        s.attack += 1;
    }
    if (infantry && has(Tech::ScaleMailArmor))
        || (kind == UnitKind::Scout && has(Tech::ScaleBardingArmor))
        || (archer && has(Tech::PaddedArcherArmor))
    {
        s.melee_armour += 1;
        s.pierce_armour += 1;
    }
    if archer && has(Tech::Fletching) {
        s.attack += 1;
        s.range += 1;
        s.los += 1;
    }
    s
}

pub fn class_bonus(attacker: UnitKind, victim: UnitKind) -> u16 {
    match (attacker, victim) {
        (UnitKind::Spearman, UnitKind::Scout) => 15, // DE armour class 8, cavalry
        (UnitKind::Skirmisher, UnitKind::Archer | UnitKind::Skirmisher) => 3, // class 15
        (UnitKind::Skirmisher, UnitKind::Spearman) => 3, // class 27, spear line
        (UnitKind::Archer, UnitKind::Spearman) => 3,
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
            // TC shoots only when garrisoned in DE; no garrison command exists yet.
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
        if self.has_researched(player, Tech::Fletching) {
            s.attack += 1;
            if kind != BuildingKind::TownCenter {
                s.range += 1;
            }
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
