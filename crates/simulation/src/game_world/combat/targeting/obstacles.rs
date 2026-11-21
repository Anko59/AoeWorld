use super::*;

impl GameWorld {
    /// A narrow firing window or a partial bucket sample cannot rule out
    /// other enemies in sight. Defer the fallback if the shared budget runs out.
    pub(super) fn preferred_target_in_sight(&mut self, area: &TargetArea) -> Option<bool> {
        let sight = TargetArea {
            min: 0,
            max: area.sight,
            ..*area
        };
        let bounds = sight.bounds(self);
        for y in bounds.min.y..bounds.max.y {
            for x in bounds.min.x..bounds.max.x {
                let tile = aoe_core::TileCoord::new(x, y);
                if self
                    .building_at_tile(tile)
                    .and_then(|id| self.building(id))
                    .is_some_and(|b| {
                        b.player != sight.player
                            && b.hp > 0
                            && !b.kind.is_obstacle_target()
                            && b.kind != BuildingKind::Bridge
                            && sight.accepts(Goal::Building(b.footprint))
                            && self.building_visible_to(sight.player, b.footprint)
                    })
                {
                    return Some(true);
                }
            }
        }
        let inspections = self.troop_inspections_available();
        let mut scanned = 0;
        for y in bounds.min.y.div_euclid(SPATIAL_CHUNK_TILES)
            ..=(bounds.max.y - 1).div_euclid(SPATIAL_CHUNK_TILES)
        {
            for x in bounds.min.x.div_euclid(SPATIAL_CHUNK_TILES)
                ..=(bounds.max.x - 1).div_euclid(SPATIAL_CHUNK_TILES)
            {
                let Some(bucket) = self.chunks.get(&ChunkCoord { x, y }) else {
                    continue;
                };
                for &id in bucket {
                    if scanned == inspections {
                        self.combat.stats.targeting_scanned += scanned;
                        return None;
                    }
                    scanned += 1;
                    if self.unit(id).is_some_and(|u| {
                        u.player != sight.player
                            && u.hp > 0
                            && (!sight.ram || u.kind.is_siege())
                            && sight.accepts(Goal::Unit(u.position, u.kind.collision_radius()))
                            && self.unit_visible_to(sight.player, id)
                    }) {
                        self.combat.stats.targeting_scanned += scanned;
                        return Some(true);
                    }
                }
            }
        }
        self.combat.stats.targeting_scanned += scanned;
        Some(false)
    }
}
