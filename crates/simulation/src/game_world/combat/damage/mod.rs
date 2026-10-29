//! Post-armour elevation and exact quarter-point health, shared by all hits.
use super::{DamageKind, HitRecord, Projectile};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, Tick};

struct HitPower {
    attack: u16,
    bonus: u16,
    kind: DamageKind,
    elevation: bool,
}

impl GameWorld {
    pub(in crate::game_world) fn resolve_hit(
        &mut self,
        source: (EntityId, PlayerId),
        target: EntityId,
        attack: u16,
        bonus: u16,
        kind: DamageKind,
        changed: &mut Vec<EntityId>,
    ) {
        self.resolve_damage(
            source,
            target,
            HitPower {
                attack,
                bonus,
                kind,
                elevation: true,
            },
            changed,
        );
    }

    pub(super) fn resolve_projectile_hit(&mut self, p: Projectile, changed: &mut Vec<EntityId>) {
        self.resolve_damage(
            (p.attacker, p.player),
            p.target,
            HitPower {
                attack: p.attack,
                bonus: p.bonus,
                kind: DamageKind::Pierce,
                elevation: !p.secondary,
            },
            changed,
        );
    }

    fn combat_height(&self, id: EntityId) -> Option<i16> {
        let crate::terrain::Terrain::Map { generator, .. } = &self.terrain else {
            return Some(0);
        };
        let tile = self
            .unit(id)
            .filter(|u| u.hp > 0)
            .map(|u| u.position.tile_floor())
            .or_else(|| {
                self.building(id).filter(|b| b.hp > 0).map(|b| {
                    super::tick::Goal::Building(b.footprint)
                        .centre()
                        .tile_floor()
                })
            })
            .or_else(|| {
                self.animal(id)
                    .filter(|a| a.hp > 0)
                    .map(|a| a.position.tile_floor())
            })?;
        generator.tile_at(tile).map(|tile| tile.game_height_level)
    }

    fn apply_combat_damage(
        &mut self,
        attacker: EntityId,
        target: EntityId,
        hp: u16,
        raw: u16,
        elevation: bool,
    ) -> u16 {
        // DE multiplies total damage, not attack, and applies the minimum last.
        // Only living primary shooters contribute elevation at impact.
        let factor = if elevation && self.terrain.has_map_navigation() {
            match (self.combat_height(attacker), self.combat_height(target)) {
                (Some(a), Some(v)) => match a.cmp(&v) {
                    std::cmp::Ordering::Greater => 5,
                    std::cmp::Ordering::Less => 3,
                    std::cmp::Ordering::Equal => 4,
                },
                _ => 4,
            }
        } else {
            4
        };
        let damage = (u32::from(raw) * factor).max(4);
        let quarter = &mut self.combat.hp_quarters[target.0 as usize];
        let left = (u32::from(hp) * 4 + u32::from(*quarter)).saturating_sub(damage);
        let hp = (left / 4) as u16;
        *quarter = if hp == 0 { 0 } else { (left % 4) as u8 };
        hp
    }

    /// Both melee and tower arrows share armour, death and retaliation logic.
    fn resolve_damage(
        &mut self,
        source: (EntityId, PlayerId),
        target: EntityId,
        power: HitPower,
        changed: &mut Vec<EntityId>,
    ) {
        let (attacker_id, attacker_player) = source;
        let HitPower {
            attack,
            bonus,
            kind,
            ..
        } = power;
        let hit = HitRecord {
            attacker: attacker_id,
            tick: Tick(self.tick.0),
        };
        self.combat.stats.hits += 1;
        if let Some(&victim_index) = self.lookup.get(&target) {
            if self.units[victim_index].state.hp == 0 || self.embarked(target) {
                return;
            }
            let (melee, pierce) = self.unit_armour(target).unwrap_or_default();
            let raw = super::table::damage_components(attack, kind, melee, pierce, bonus);
            let hp = self.apply_combat_damage(
                source.0,
                target,
                self.units[victim_index].state.hp,
                raw,
                power.elevation,
            );
            let victim = &mut self.units[victim_index].state;
            victim.hp = hp;
            victim.last_hit = Some(hit);
            let (hp, victim_player) = (victim.hp, victim.player);
            self.record_combat_hit(
                Some(attacker_player),
                victim_player,
                kind == super::DamageKind::Pierce,
                hp == 0,
            );
            if hp == 0 {
                self.kill_unit(victim_index);
                self.finish_target_attack(attacker_id, target, changed);
            } else if attacker_player != victim_player {
                self.retaliate(victim_index, attacker_id, changed);
            }
            changed.push(self.units[victim_index].state.id);
        } else if let Some(animal) = self.animal(target) {
            let before = animal.hp;
            let hp = self.apply_combat_damage(attacker_id, target, before, attack, power.elevation);
            if self.strike_animal(target, attacker_id, before - hp) {
                self.finish_target_attack(attacker_id, target, changed);
            }
        } else if let Some(building) = self.building(target) {
            let armour = super::table::age_armour(building.kind, building.age);
            let masonry = self.building_has_masonry(building);
            let raw = super::table::damage_components(
                attack,
                kind,
                armour.melee_armour + i16::from(masonry),
                armour.pierce_armour + u16::from(masonry),
                bonus,
            );
            let hp =
                self.apply_combat_damage(attacker_id, target, building.hp, raw, power.elevation);
            let Some(building) = self.economy.buildings.get_mut(&target) else {
                return;
            };
            building.hp = hp;
            building.last_hit = Some(hit);
            let occupants: Vec<_> = building.garrison.iter().map(|g| g.unit.id).collect();
            let (building_kind, footprint) = (building.kind, building.footprint);
            let (player, killed) = (building.player, building.hp == 0);
            self.record_combat_hit(
                Some(attacker_player),
                player,
                kind == super::DamageKind::Pierce,
                killed,
            );
            if killed {
                // Ejection restores map units during this hit. Include their
                // identities in the ordinary delta for simulation consumers.
                changed.extend(occupants);
                if building_kind == crate::BuildingKind::Bridge {
                    let rect = footprint;
                    changed.extend(self.query(rect).0.into_iter().map(|u| u.id));
                }
                self.destroy_building(target);
                self.finish_target_attack(attacker_id, target, changed);
            }
        }
    }
}
