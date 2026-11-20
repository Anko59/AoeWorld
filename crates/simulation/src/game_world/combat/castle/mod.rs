//! Local Castle mechanics, using the existing unit spatial index.
use crate::{GameWorld, Projectile, UnitKind};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, TileCoord, TileRect, WorldPosition};
mod rest;
pub(crate) use rest::Rest;

impl GameWorld {
    pub(in crate::game_world::combat) fn resolve_splash(
        &mut self,
        p: Projectile,
        changed: &mut Vec<EntityId>,
    ) {
        // Aim remains fixed at launch. The blast circle includes friendly fire;
        // the inner half of the radius takes full damage, the rest half.
        let source = p.source.unwrap_or(UnitKind::Mangonel);
        let kind = Self::artillery_damage_kind(p.source);
        let scatter = p.is_secondary_stone();
        let hundredths = if scatter {
            1 // DAT projectile 369: 0.01 tiles and no damage classes.
        } else {
            i64::from(super::siege::blast_hundredths(source))
        };
        if hundredths == 0 {
            // A Trebuchet boulder hits only what stands at its aim point.
            let hit = self
                .unit(p.target)
                .map_or(self.building(p.target).is_some(), |u| {
                    let dx = i64::from(u.position.x) - i64::from(p.to.x);
                    let dy = i64::from(u.position.y) - i64::from(p.to.y);
                    dx * dx + dy * dy <= i64::from(FIXED_SUBUNITS_PER_TILE / 2).pow(2)
                        && !self.projectile_misses(&p)
                });
            if hit {
                self.resolve_hit(
                    (p.attacker, p.player),
                    p.target,
                    p.attack,
                    p.bonus,
                    kind,
                    changed,
                );
            }
            return;
        }
        let radius = i64::from(FIXED_SUBUNITS_PER_TILE) * hundredths / 100;
        let radius_i32 = radius as i32;
        let min = WorldPosition::new(p.to.x - radius_i32, p.to.y - radius_i32).tile_floor();
        let max = WorldPosition::new(p.to.x + radius_i32, p.to.y + radius_i32).tile_floor();
        let bounds = TileRect::new(
            TileCoord::new(min.x.max(0), min.y.max(0)),
            TileCoord::new(
                (max.x + 1).min(self.config.width_tiles),
                (max.y + 1).min(self.config.height_tiles),
            ),
        );
        let mut hits = Vec::new();
        let chunk_size = aoe_core::SPATIAL_CHUNK_TILES;
        for y in bounds.min.y / chunk_size..=(bounds.max.y - 1) / chunk_size {
            for x in bounds.min.x / chunk_size..=(bounds.max.x - 1) / chunk_size {
                let Some(ids) = self.chunks.get(&aoe_core::ChunkCoord { x, y }) else {
                    continue;
                };
                hits.extend(ids.iter().filter_map(|&id| {
                    let u = self.unit(id)?;
                    let dx = i64::from(u.position.x) - i64::from(p.to.x);
                    let dy = i64::from(u.position.y) - i64::from(p.to.y);
                    let d = dx * dx + dy * dy;
                    (d <= radius * radius && u.hp > 0 && !self.embarked(id)).then_some((id, d))
                }));
            }
        }
        hits.sort_unstable_by_key(|&(id, _)| id);
        let mut hit = |world: &mut GameWorld, id, divisor| {
            if scatter {
                // DAT projectile 369 has no attack entries: its stones are
                // seen landing but deal no damage, not even the floor.
                return;
            }
            let bonus = if id == p.target {
                p.bonus
            } else {
                world.unit_damage_bonus(source, id)
            };
            world.resolve_hit(
                (p.attacker, p.player),
                id,
                p.attack / divisor,
                bonus / divisor,
                kind,
                changed,
            );
        };
        for (id, d) in hits {
            hit(self, id, if d <= radius * radius / 4 { 1 } else { 2 });
        }
        let mut buildings = std::collections::BTreeSet::new();
        // A footprint ending at the lower bound can touch the blast circle.
        for y in (bounds.min.y - 1).max(0)..bounds.max.y {
            for x in (bounds.min.x - 1).max(0)..bounds.max.x {
                if let Some(id) = self.building_at(TileCoord::new(x, y)) {
                    buildings.insert(id);
                }
            }
        }
        for id in buildings {
            let Some(b) = self.building(id) else {
                continue;
            };
            let distance = super::tick::Goal::Building(b.footprint).distance_squared(p.to);
            if distance <= radius * radius {
                hit(self, id, 1);
            }
        }
    }
    pub(in crate::game_world::combat) fn advance_healing(&mut self, changed: &mut Vec<EntityId>) {
        // 2.5 HP/s at the standard 20 Hz, rounded to a tick for slower demo clocks.
        let period = (u64::from(self.config.tick_hz) * 2).div_ceil(5).max(1);
        if !self.tick.0.is_multiple_of(period) {
            return;
        }
        #[cfg(test)]
        self.assert_resting_healers();
        // Cache only wounded, healable ids, once per touched spatial bucket.
        // Healthy crowds are scanned once rather than sorted once per Monk.
        let mut wounded = std::collections::BTreeMap::new();
        let healers = self
            .combat
            .healing
            .ready(self.economy.no_sleep, &self.combat.healers);
        for id in healers {
            let Some(m) = self.unit(id).filter(|u| u.hp > 0) else {
                self.combat.healers.remove(&id);
                self.combat.healing.remove(id);
                continue;
            };
            if self.embarked(id) || m.moving || m.planning || m.attack_target.is_some() {
                if !self.economy.no_sleep {
                    self.combat.healing.park(id);
                }
                continue;
            }
            let tile = m.position.tile_floor();
            let radius = 4;
            let bounds = TileRect::new(
                TileCoord::new((tile.x - radius).max(0), (tile.y - radius).max(0)),
                TileCoord::new(
                    (tile.x + radius + 1).min(self.config.width_tiles),
                    (tile.y + radius + 1).min(self.config.height_tiles),
                ),
            );
            let chunk = |x: i32, y: i32| aoe_core::ChunkCoord {
                x: x.div_euclid(aoe_core::SPATIAL_CHUNK_TILES),
                y: y.div_euclid(aoe_core::SPATIAL_CHUNK_TILES),
            };
            let first = chunk(bounds.min.x, bounds.min.y);
            let last = chunk(bounds.max.x - 1, bounds.max.y - 1);
            let mut target = None;
            for y in first.y..=last.y {
                for x in first.x..=last.x {
                    let candidates =
                        wounded
                            .entry(aoe_core::ChunkCoord { x, y })
                            .or_insert_with(|| {
                                let Some(bucket) = self.chunks.get(&aoe_core::ChunkCoord { x, y })
                                else {
                                    return Vec::new();
                                };
                                self.combat.stats.healing_scanned_units += bucket.len() as u64;
                                bucket
                                    .iter()
                                    .filter_map(|id| self.unit(*id))
                                    .filter(|u| {
                                        u.hp > 0
                                            && u.hp < u.max_hp
                                            && !self.embarked(u.id)
                                            && !u.kind.is_naval()
                                            && !u.kind.is_siege()
                                    })
                                    .map(|u| u.id)
                                    .collect::<Vec<_>>()
                            });
                    self.combat.stats.healing_candidates += candidates.len() as u64;
                    for &candidate in candidates.iter() {
                        // Earlier Monks can fill a target's HP during this same tick.
                        let Some(u) = self.unit(candidate).filter(|u| {
                            u.player == m.player && u.hp > 0 && u.hp < u.max_hp && u.id != id
                        }) else {
                            continue;
                        };
                        let dx = i64::from(u.position.x) - i64::from(m.position.x);
                        let dy = i64::from(u.position.y) - i64::from(m.position.y);
                        let d = dx * dx + dy * dy;
                        if d <= i64::from(radius * FIXED_SUBUNITS_PER_TILE).pow(2) {
                            target = Some(
                                target.map_or((d, u.id), |old: (i64, EntityId)| old.min((d, u.id))),
                            );
                        }
                    }
                }
            }
            if let Some((_, target)) = target
                && let Some(&i) = self.lookup.get(&target)
            {
                self.units[i].state.hp += 1;
                if self.units[i].state.hp == self.units[i].state.max_hp {
                    self.combat.hp_quarters[target.0 as usize] = 0;
                }
                changed.push(target);
            } else if !self.economy.no_sleep {
                self.combat.healing.sleep(&m, bounds);
            }
        }
    }
}
