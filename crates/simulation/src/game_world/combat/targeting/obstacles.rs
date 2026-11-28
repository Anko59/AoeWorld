use super::*;

impl GameWorld {
    /// A narrow firing window or a partial bucket sample cannot rule out
    /// other enemies in sight: this reads every visible enemy in sight.
    pub(super) fn preferred_target_in_sight(&mut self, area: &TargetArea) -> bool {
        let sight = TargetArea {
            min: 0,
            max: area.sight,
            ..*area
        };
        let bounds = sight.bounds(self);
        let rows = if self.building_cells_near(bounds, sight.player) {
            bounds.min.y..bounds.max.y
        } else {
            0..0
        };
        for y in rows {
            for x in bounds.min.x..bounds.max.x {
                let tile = aoe_core::TileCoord::new(x, y);
                if self
                    .building_at_tile(tile)
                    .and_then(|id| self.building(id))
                    .is_some_and(|b| {
                        self.is_enemy(sight.player, b.player)
                            && b.hp > 0
                            && !b.kind.is_obstacle_target()
                            && b.kind != BuildingKind::Bridge
                            && sight.accepts(Goal::Building(b.footprint))
                            && self.building_visible_to(sight.player, b.footprint)
                    })
                {
                    return true;
                }
            }
        }
        for y in bounds.min.y.div_euclid(SPATIAL_CHUNK_TILES)
            ..=(bounds.max.y - 1).div_euclid(SPATIAL_CHUNK_TILES)
        {
            for x in bounds.min.x.div_euclid(SPATIAL_CHUNK_TILES)
                ..=(bounds.max.x - 1).div_euclid(SPATIAL_CHUNK_TILES)
            {
                let chunk = ChunkCoord { x, y };
                if !self.enemies_present(&self.chunks.presence, chunk, sight.player) {
                    continue;
                }
                let mut found = false;
                self.each_enemy_contact(chunk, sight.player, bounds, |world, c| {
                    found = found
                        || ((!sight.ram || c.siege)
                            && sight.accepts(Goal::Unit(c.position, c.radius))
                            && world.unit(c.id).is_some_and(|u| {
                                world.is_enemy(sight.player, u.player) && u.hp > 0
                            })
                            && world.unit_visible_to(sight.player, c.id));
                });
                if found {
                    return true;
                }
            }
        }
        false
    }
}
