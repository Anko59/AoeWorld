//! Local Castle mechanics, using the existing unit spatial index.
use crate::{GameWorld, Projectile, UnitKind};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, TileCoord, TileRect};

impl GameWorld {
    pub(in crate::game_world::combat) fn resolve_splash(
        &mut self,
        p: Projectile,
        changed: &mut Vec<EntityId>,
    ) {
        // Aim remains fixed at launch. A one-tile circle includes friendly fire;
        // integer distance bands give full then half damage.
        let radius = i64::from(FIXED_SUBUNITS_PER_TILE);
        let tile = p.to.tile_floor();
        let bounds = TileRect::new(
            TileCoord::new((tile.x - 2).max(0), (tile.y - 2).max(0)),
            TileCoord::new(
                (tile.x + 3).min(self.config.width_tiles),
                (tile.y + 3).min(self.config.height_tiles),
            ),
        );
        let hits: Vec<_> = self
            .query(bounds)
            .0
            .into_iter()
            .filter_map(|u| {
                let dx = i64::from(u.position.x) - i64::from(p.to.x);
                let dy = i64::from(u.position.y) - i64::from(p.to.y);
                let d = dx * dx + dy * dy;
                (u.hp > 0 && d <= radius * radius).then_some((u.id, d))
            })
            .collect();
        for (id, d) in hits {
            let unit = i64::from(FIXED_SUBUNITS_PER_TILE);
            let divisor = if d <= unit * unit / 4 { 1 } else { 2 };
            let bonus = self.unit_damage_bonus(UnitKind::Mangonel, id);
            self.resolve_hit(
                (p.attacker, p.player),
                id,
                p.attack / divisor,
                bonus / divisor,
                crate::DamageKind::Melee,
                changed,
            );
        }
        let mut buildings = std::collections::BTreeSet::new();
        for y in bounds.min.y..bounds.max.y {
            for x in bounds.min.x..bounds.max.x {
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
                self.resolve_hit(
                    (p.attacker, p.player),
                    id,
                    p.attack,
                    self.unit_damage_bonus(UnitKind::Mangonel, id),
                    crate::DamageKind::Melee,
                    changed,
                );
            }
        }
    }
    pub(in crate::game_world::combat) fn advance_healing(&mut self, changed: &mut Vec<EntityId>) {
        // 2.5 HP/s at the standard 20 Hz, rounded to a tick for slower demo clocks.
        let period = (u64::from(self.config.tick_hz) * 2).div_ceil(5).max(1);
        if !self.tick.0.is_multiple_of(period) {
            return;
        }
        // Cache only wounded, healable ids, once per touched spatial bucket.
        // Healthy crowds are scanned once rather than sorted once per Monk.
        let mut wounded = std::collections::BTreeMap::new();
        let healers: Vec<_> = self.combat.healers.iter().copied().collect();
        for id in healers {
            let Some(m) = self.unit(id).filter(|u| u.hp > 0) else {
                self.combat.healers.remove(&id);
                continue;
            };
            if self.embarked(id) || m.moving || m.planning || m.attack_target.is_some() {
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
                                            && !matches!(
                                                u.kind,
                                                UnitKind::Mangonel | UnitKind::BatteringRam
                                            )
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
                changed.push(target);
            }
        }
    }
}
