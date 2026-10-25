//! Defensive buildings share indexed targeting and the existing arrow system.
use crate::{BuildingKind, GameWorld};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, TileCoord, TileRect, WorldPosition};

impl GameWorld {
    /// Garrison firepower / single-arrow firepower, floored after summing.
    /// Villagers supply 5 damage / 2 s; foot archers use researched pierce DPS.
    /// Original rules: TC 0..10, tower 1..5, Castle 5..21 arrows.
    pub fn building_arrow_count(&self, id: EntityId) -> u8 {
        let Some(b) = self.building(id).filter(|b| b.is_complete() && b.hp > 0) else {
            return 0;
        };
        let Some(arrow) = self.building_attack(b.player, b.kind) else {
            return 0;
        };
        let (base, maximum) = match b.kind {
            BuildingKind::TownCenter => (0, 10),
            BuildingKind::WatchTower => (1, 5),
            BuildingKind::Castle => (5, 21),
            _ => return 0,
        };
        let researched = self.player_economy(b.player).map_or(0, |p| p.researched);
        // All current eligible reload periods divide 600 centiseconds exactly.
        let power: u32 = b
            .garrison
            .iter()
            .map(|g| match g.unit.kind {
                crate::UnitKind::Villager => {
                    3 * (5
                        + u32::from(self.has_researched(b.player, crate::Tech::Fletching))
                        + u32::from(self.has_researched(b.player, crate::Tech::BodkinArrow)))
                }
                crate::UnitKind::Archer
                | crate::UnitKind::Skirmisher
                | crate::UnitKind::Crossbowman
                | crate::UnitKind::EliteSkirmisher => {
                    let stats =
                        super::researched_stats(g.unit.kind, self.is_feudal(b.player), researched);
                    u32::from(stats.attack) * 600 / stats.attack_period_centiseconds
                }
                _ => 0,
            })
            .sum();
        (base + power / (u32::from(arrow.attack) * 600 / arrow.period_centiseconds)).min(maximum)
            as u8
    }

    pub(super) fn advance_towers(&mut self, _changed: &mut Vec<EntityId>) {
        // Registered defensive buildings query spatial buckets, never all units.
        let towers: Vec<_> = self.combat.tower_ready.keys().copied().collect();
        for id in towers {
            let Some(tower) = self
                .building(id)
                .filter(|b| b.kind.garrison_capacity() > 0 && b.is_complete() && b.hp > 0)
            else {
                self.combat.tower_ready.remove(&id);
                continue;
            };
            if self.combat.tower_ready[&id] > self.tick.0 {
                continue;
            }
            let arrows = self.building_arrow_count(id);
            if arrows == 0 {
                continue;
            }
            let Some(arrow) = self.building_attack(tower.player, tower.kind) else {
                continue;
            };
            let footprint = tower.footprint;
            let radius = i32::from(arrow.range);
            let tower_kind = tower.kind;
            let player = tower.player;
            let at = WorldPosition::new(
                (footprint.min.x + footprint.max.x) * FIXED_SUBUNITS_PER_TILE / 2,
                (footprint.min.y + footprint.max.y) * FIXED_SUBUNITS_PER_TILE / 2,
            );
            let bounds = TileRect::new(
                TileCoord::new(
                    (footprint.min.x - radius).max(0),
                    (footprint.min.y - radius).max(0),
                ),
                TileCoord::new(
                    (footprint.max.x + radius + 1).min(self.config.width_tiles),
                    (footprint.max.y + radius + 1).min(self.config.height_tiles),
                ),
            );
            let square = i64::from(FIXED_SUBUNITS_PER_TILE).pow(2);
            let target = self
                .query(bounds)
                .0
                .into_iter()
                .filter(|u| u.player != player && u.hp > 0 && self.unit_visible_to(player, u.id))
                .filter_map(|u| {
                    let dx = i64::from(u.position.x) - i64::from(at.x);
                    let dy = i64::from(u.position.y) - i64::from(at.y);
                    let distance = if tower_kind == BuildingKind::Castle {
                        super::tick::Goal::Building(footprint).distance_squared(u.position)
                    } else {
                        dx * dx + dy * dy
                    };
                    (distance >= i64::from(arrow.min_range).pow(2) * square
                        && distance <= i64::from(arrow.range).pow(2) * square)
                        .then_some((distance, u.id))
                })
                .min();
            if let Some((_, target)) = target {
                self.combat.tower_ready.insert(
                    id,
                    self.tick.0
                        + (u64::from(arrow.period_centiseconds) * u64::from(self.config.tick_hz))
                            .div_ceil(100),
                );
                let bonus = self.unit(target).map_or(0, |u| {
                    if matches!(u.kind, crate::UnitKind::Spearman | crate::UnitKind::Pikeman) {
                        2
                    } else {
                        0
                    }
                });
                for _ in 0..arrows {
                    self.launch_projectile(
                        id,
                        target,
                        arrow.attack,
                        bonus,
                        super::ProjectileKind::Arrow,
                    );
                }
            }
        }
    }
}
