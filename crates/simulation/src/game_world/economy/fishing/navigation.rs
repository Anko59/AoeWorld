use crate::{GameWorld, UnitKind};
use aoe_core::TileCoord;
use aoe_map::{EnvironmentPageError, ResourceOverlay, RouteTerrain};

/// Water shared by every ship search, including later polls and orders.
/// Collision notifications invalidate only changed chunks of derived ground.
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
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if let Some(known) = self
            .world
            .economy
            .naval_ground
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(tile)
        {
            return Ok(known);
        }
        let wet = !self.world.economy.blocked_tiles.contains(&tile)
            && self
                .world
                .unit_tile_passable(UnitKind::FishingShip, tile, cancelled)?;
        self.world
            .economy
            .naval_ground
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(tile, wet);
        Ok(wet)
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
        if let Some(clear) = self
            .world
            .economy
            .naval_ground
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_edge(from, to)
        {
            return Ok(clear);
        }
        let wet = |tile| self.wet(tile, cancelled);
        let clear = wet(from)?
            && wet(to)?
            && (from.x == to.x
                || from.y == to.y
                || (wet(TileCoord::new(from.x, to.y))? && wet(TileCoord::new(to.x, from.y))?));
        self.world
            .economy
            .naval_ground
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert_edge(from, to, clear);
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
        if !kind.is_naval() {
            return self
                .terrain
                .passable_with_cancel(to, self.config, &|| false)
                .and_then(|passable| {
                    if passable && from != to {
                        self.terrain
                            .crossable_with_cancel(from, to, self.config, &|| false)
                    } else {
                        Ok(passable)
                    }
                });
        }
        let wet = |tile| self.unit_tile_passable(kind, tile, &|| false);
        Ok(wet(to)?
            && (from.x == to.x
                || from.y == to.y
                || (wet(TileCoord::new(from.x, to.y))? && wet(TileCoord::new(to.x, from.y))?)))
    }
}
