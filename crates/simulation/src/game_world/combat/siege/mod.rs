//! Castle- and Imperial-Age siege and gunpowder mechanics: blast widths,
//! accuracy, Scorpion pass-through bolts, Petard detonation and Trebuchet
//! packing. Numbers: aoe2techtree data.json (DE update 188329) `BlastWidth`
//! and `AccuracyPercent` of DAT units 280, 550, 588, 36, 42, 440, 39, 474, 5.
use super::{Projectile, unit_combat};
use crate::{DamageKind, GameWorld, UnitKind};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, TileRect, WorldPosition};

#[cfg(test)]
mod tests;
mod timing;
pub use timing::native_attack_windup_millis;

/// DAT transform work (50) and work rate (4.5/s) of units 42 and 331.
pub const TREBUCHET_TRANSFORM_WORK: u32 = 50;
pub const TREBUCHET_WORK_PER_SECOND_TENTHS: u32 = 45;

pub const fn trebuchet_transform_ticks(hz: u32) -> u64 {
    (hz as u64 * TREBUCHET_TRANSFORM_WORK as u64 * 10)
        .div_ceil(TREBUCHET_WORK_PER_SECOND_TENTHS as u64)
}

/// Blast radius in hundredths of a tile: DE's `BlastWidth` around the impact.
pub(crate) const fn blast_hundredths(kind: UnitKind) -> u32 {
    match kind {
        UnitKind::CannonGalleon => 25,
        UnitKind::EliteCannonGalleon | UnitKind::CatapultGalleon => 30,
        UnitKind::Mangonel => 100,
        UnitKind::Onager => 125,
        UnitKind::SiegeOnager => 150,
        UnitKind::BombardCannon | UnitKind::Petard => 50,
        _ => 0,
    }
}

/// DE accuracy against units. Older roster units keep the historical
/// always-hit rule; the new missile units use their DAT accuracy.
pub(crate) const fn accuracy_percent(kind: UnitKind) -> u64 {
    match kind {
        UnitKind::CavalryArcher => 50,
        UnitKind::HeavyCavalryArcher => 80,
        UnitKind::HandCannoneer => 75,
        UnitKind::TrebuchetUnpacked => 15,
        _ => 100,
    }
}

impl GameWorld {
    /// Deterministic accuracy roll for a projectile hitting a unit.
    pub(super) fn projectile_misses(&self, p: &Projectile) -> bool {
        let Some(source) = p.source else {
            return false;
        };
        let accuracy = accuracy_percent(source);
        if accuracy >= 100 || self.unit(p.target).is_none() {
            return false;
        }
        super::projectiles::projectile_roll(p.id, p.attacker) % 100 >= accuracy
    }

    /// A Scorpion bolt damages every enemy unit on its line, from the launch
    /// point through the aim point and one tile beyond it.
    pub(super) fn resolve_pass_through(&mut self, p: Projectile, changed: &mut Vec<EntityId>) {
        let tile = i64::from(FIXED_SUBUNITS_PER_TILE);
        let (fx, fy) = (i64::from(p.from.x), i64::from(p.from.y));
        let (mut tx, mut ty) = (i64::from(p.to.x), i64::from(p.to.y));
        let (dx, dy) = (tx - fx, ty - fy);
        let length = (dx * dx + dy * dy).isqrt().max(1);
        tx += dx * tile / length;
        ty += dy * tile / length;
        let to_tile = |v: i64, limit: i32| (v.div_euclid(tile) as i32).clamp(0, limit);
        let (w, h) = (self.config.width_tiles, self.config.height_tiles);
        let (x0, x1) = (to_tile(fx.min(tx), w), to_tile(fx.max(tx), w));
        let (y0, y1) = (to_tile(fy.min(ty), h), to_tile(fy.max(ty), h));
        let rect = TileRect::from_xywh(x0, y0, (x1 + 1).min(w) - x0, (y1 + 1).min(h) - y0);
        let (sx, sy) = (tx - fx, ty - fy);
        let span = (sx * sx + sy * sy).max(1);
        let reach = tile * 3 / 10;
        let mut hits: Vec<EntityId> = self
            .query(rect)
            .0
            .into_iter()
            .filter(|u| u.hp > 0 && u.player != p.player)
            .filter(|u| {
                let (px, py) = (i64::from(u.position.x) - fx, i64::from(u.position.y) - fy);
                let along = (px * sx + py * sy).clamp(0, span);
                let cx = fx + sx * along / span - i64::from(u.position.x);
                let cy = fy + sy * along / span - i64::from(u.position.y);
                cx * cx + cy * cy <= reach * reach
            })
            .map(|u| u.id)
            .collect();
        if !hits.contains(&p.target) && self.building(p.target).is_some() {
            hits.push(p.target);
        }
        hits.sort_unstable();
        let source = p.source.unwrap_or(UnitKind::Scorpion);
        for id in hits {
            let bonus = if id == p.target {
                p.bonus
            } else {
                self.unit_damage_bonus(source, id)
            };
            self.resolve_hit(
                (p.attacker, p.player),
                id,
                p.attack,
                bonus,
                DamageKind::Pierce,
                changed,
            );
        }
    }

    /// The Petard explodes on contact: every enemy unit and building within
    /// its blast takes its attack and class bonuses, then the Petard dies.
    pub(super) fn detonate_petard(
        &mut self,
        attacker: EntityId,
        target: EntityId,
        attack: u16,
        changed: &mut Vec<EntityId>,
    ) {
        let Some(petard) = self.unit(attacker) else {
            return;
        };
        let player = petard.player;
        if self.attack_goal(target, player).is_none() {
            return;
        }
        let centre = petard.position;
        let radius = i64::from(FIXED_SUBUNITS_PER_TILE)
            * i64::from(blast_hundredths(UnitKind::Petard))
            / 100
            + i64::from(FIXED_SUBUNITS_PER_TILE) / 2;
        let mut targets = vec![target];
        targets.extend(self.blast_victims(player, centre, radius));
        targets.sort_unstable();
        targets.dedup();
        for id in targets {
            let bonus = self.unit_damage_bonus(UnitKind::Petard, id);
            self.resolve_hit(
                (attacker, player),
                id,
                attack,
                bonus,
                DamageKind::Melee,
                changed,
            );
        }
        if let Some(&index) = self.lookup.get(&attacker) {
            self.kill_unit(index);
            changed.push(attacker);
        }
    }

    /// Enemy units and buildings within `radius` subunits of `centre`.
    fn blast_victims(&self, player: PlayerId, centre: WorldPosition, radius: i64) -> Vec<EntityId> {
        let tile = centre.tile_floor();
        let span = (radius / i64::from(FIXED_SUBUNITS_PER_TILE)) as i32 + 2;
        let (w, h) = (self.config.width_tiles, self.config.height_tiles);
        let (x0, y0) = ((tile.x - span).max(0), (tile.y - span).max(0));
        let rect = TileRect::from_xywh(
            x0,
            y0,
            (tile.x + span + 1).min(w) - x0,
            (tile.y + span + 1).min(h) - y0,
        );
        let mut out: Vec<EntityId> = self
            .query(rect)
            .0
            .into_iter()
            .filter(|u| u.hp > 0 && u.player != player)
            .filter(|u| {
                let dx = i64::from(u.position.x) - i64::from(centre.x);
                let dy = i64::from(u.position.y) - i64::from(centre.y);
                dx * dx + dy * dy <= radius * radius
            })
            .map(|u| u.id)
            .collect();
        let mut buildings = std::collections::BTreeSet::new();
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                if let Some(id) = self.building_at(aoe_core::TileCoord::new(x, y)) {
                    buildings.insert(id);
                }
            }
        }
        out.extend(buildings.into_iter().filter(|&id| {
            self.building(id).is_some_and(|b| {
                b.player != player
                    && b.hp > 0
                    && super::tick::Goal::Building(b.footprint).distance_squared(centre)
                        <= radius * radius
            })
        }));
        out
    }

    /// Selects the destination form and starts the stationary transform.
    pub(super) fn set_trebuchet_packed(&mut self, index: usize, packed: bool) {
        let state = &self.units[index].state;
        let (from, to) = if packed {
            (UnitKind::TrebuchetUnpacked, UnitKind::Trebuchet)
        } else {
            (UnitKind::Trebuchet, UnitKind::TrebuchetUnpacked)
        };
        if state.kind != from {
            return;
        }
        let (id, player) = (state.id, state.player);
        self.units[index].state.kind = to;
        self.change_unit_census(player, Some(from), Some(to), 1);
        self.combat.trebuchets_transforming.insert(
            id,
            self.tick
                .0
                .saturating_add(trebuchet_transform_ticks(self.config.tick_hz)),
        );
    }

    /// Any move order packs an unpacked Trebuchet first.
    pub(crate) fn pack_for_move(&mut self, index: usize) {
        if self.units[index].state.kind == UnitKind::TrebuchetUnpacked {
            self.set_trebuchet_packed(index, true);
        }
    }

    pub fn trebuchet_transform_until(&self, id: EntityId) -> Option<aoe_core::Tick> {
        self.combat
            .trebuchets_transforming
            .get(&id)
            .filter(|&&until| until > self.tick.0)
            .copied()
            .map(aoe_core::Tick)
    }

    pub(crate) fn trebuchet_transforming(&self, id: EntityId) -> bool {
        self.trebuchet_transform_until(id).is_some()
    }

    pub(crate) fn trebuchet_anchored(&self, unit: &crate::GameUnit) -> bool {
        unit.kind == UnitKind::TrebuchetUnpacked
            || (unit.kind == UnitKind::Trebuchet && self.trebuchet_transforming(unit.id))
    }

    /// Routes can be prepared while packing. Hold only the transforming
    /// units out of the current movement batch.
    pub(crate) fn hold_transforming_movers(
        &mut self,
        changed: &mut Vec<EntityId>,
    ) -> Vec<EntityId> {
        let jobs = &mut self.combat.trebuchets_transforming;
        jobs.retain(|id, until| {
            if *until <= self.tick.0 {
                changed.push(*id);
                return false;
            }
            true
        });
        jobs.keys()
            .copied()
            .filter(|&id| self.active_movers.remove(id))
            .collect()
    }

    /// Damage kind of a fixed-point artillery impact: Trebuchets hit as
    /// pierce (DAT class 3), stone throwers and cannons as melee (class 4).
    pub(super) fn artillery_damage_kind(source: Option<UnitKind>) -> DamageKind {
        source.map_or(DamageKind::Melee, |k| {
            if k.is_trebuchet() {
                unit_combat(k).attack_kind
            } else {
                DamageKind::Melee
            }
        })
    }
}
