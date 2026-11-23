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
        let rows = if self.enemy_buildings_near(bounds, sight.player) {
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
                        b.player != sight.player
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
                let contacts = self.chunk_contacts(ChunkCoord { x, y }, sight.player);
                if contacts.visible.iter().any(|&id| {
                    self.unit(id).is_some_and(|u| {
                        u.player != sight.player
                            && u.hp > 0
                            && (!sight.ram || u.kind.is_siege())
                            && sight.accepts(Goal::Unit(u.position, u.kind.collision_radius()))
                    })
                }) {
                    return true;
                }
            }
        }
        false
    }
}
