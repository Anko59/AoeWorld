//! Garrison-less Watch Tower arrows. DE unit 79: 5 pierce, range 1..8, 2 s.
use crate::{BuildingKind, GameWorld};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, TileCoord, TileRect, WorldPosition};

impl GameWorld {
    pub(super) fn advance_towers(&mut self, _changed: &mut Vec<EntityId>) {
        // Only completed towers are registered; queries use the unit spatial
        // index, never an all-units scan per building. At most 256 towers.
        let towers: Vec<_> = self.combat.tower_ready.keys().copied().collect();
        for id in towers {
            let Some(tower) = self
                .building(id)
                .filter(|b| b.kind == BuildingKind::WatchTower && b.is_complete() && b.hp > 0)
            else {
                self.combat.tower_ready.remove(&id);
                continue;
            };
            if self.combat.tower_ready[&id] > self.tick.0 {
                continue;
            }
            let Some(arrow) = self.building_attack(tower.player, tower.kind) else {
                continue;
            };
            let radius = i32::from(arrow.range);
            let (player, tile) = (tower.player, tower.footprint.min);
            let at = WorldPosition::new(
                tile.x * FIXED_SUBUNITS_PER_TILE + FIXED_SUBUNITS_PER_TILE / 2,
                tile.y * FIXED_SUBUNITS_PER_TILE + FIXED_SUBUNITS_PER_TILE / 2,
            );
            let bounds = TileRect::new(
                TileCoord::new((tile.x - radius).max(0), (tile.y - radius).max(0)),
                TileCoord::new(
                    (tile.x + radius + 1).min(self.config.width_tiles),
                    (tile.y + radius + 1).min(self.config.height_tiles),
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
                    let distance = dx * dx + dy * dy;
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
                    if u.kind == crate::UnitKind::Spearman {
                        2
                    } else {
                        0
                    }
                });
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
