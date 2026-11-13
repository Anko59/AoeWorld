use crate::{GameWorld, UnitKind};
use aoe_core::TileCoord;
use aoe_map::{EnvironmentPageError, ResourceOverlay, RouteTerrain};

/// Ship searches use movement's retained physical ground. Planner reservations
/// remain separate; collision notifications invalidate ground and edge chunks.
pub(crate) struct NavalNavigation<'a> {
    world: &'a GameWorld,
}

impl<'a> NavalNavigation<'a> {
    pub(crate) fn new(world: &'a GameWorld) -> Self {
        Self { world }
    }

    fn wet(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        if self.world.economy.blocked_tiles.contains(&tile) {
            return if cancelled() {
                Err(EnvironmentPageError::Cancelled)
            } else {
                Ok(false)
            };
        }
        self.world
            .unit_tile_passable(UnitKind::FishingShip, tile, cancelled)
    }
}

impl RouteTerrain for NavalNavigation<'_> {
    fn memoize_route_queries(&self) -> bool {
        false
    }
    fn route_walkable(
        &self,
        _: &ResourceOverlay,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        self.wet(tile, cancelled)
    }
    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        // Reservations live in `blocked_tiles` and change without a collision
        // notification, so they are tested here and never stored in the shared
        // physical edge answer.
        let blocked = |tile| self.world.economy.blocked_tiles.contains(&tile);
        let diagonal = from.x != to.x && from.y != to.y;
        if blocked(from)
            || blocked(to)
            || (diagonal
                && (blocked(TileCoord::new(from.x, to.y)) || blocked(TileCoord::new(to.x, from.y))))
        {
            return Ok(false);
        }
        let clear = self
            .world
            .unit_tile_passable(UnitKind::FishingShip, from, cancelled)?
            && self
                .world
                .unit_step_clear(UnitKind::FishingShip, from, to)?;
        Ok(clear)
    }
    fn route_step_cost(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<u32, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        Ok(if from.x == to.x || from.y == to.y {
            1024
        } else {
            1448
        })
    }
}

impl GameWorld {
    pub(crate) fn unit_step_clear(
        &self,
        kind: UnitKind,
        from: TileCoord,
        to: TileCoord,
    ) -> Result<bool, EnvironmentPageError> {
        if from == to {
            return self.unit_tile_passable(kind, to, &|| false);
        }
        if matches!(self.terrain, crate::Terrain::Uniform(_)) {
            return Ok(!kind.is_naval() && self.terrain.crossable(from, to, self.config));
        }
        let cache = &self.economy.movement_ground[usize::from(kind.is_naval())];
        if let Some(clear) = cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_edge(from, to)
        {
            return Ok(clear);
        }
        let clear = if !kind.is_naval() {
            self.unit_tile_passable(kind, to, &|| false)?
                && self
                    .terrain
                    .crossable_with_cancel(from, to, self.config, &|| false)?
        } else {
            let wet = |tile| self.unit_tile_passable(kind, tile, &|| false);
            wet(to)?
                && (from.x == to.x
                    || from.y == to.y
                    || (wet(TileCoord::new(from.x, to.y))? && wet(TileCoord::new(to.x, from.y))?))
        };
        cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert_edge(from, to, clear);
        Ok(clear)
    }
}
