use crate::{GameWorld, UnitKind};
use aoe_core::TileCoord;
use aoe_map::{EnvironmentPageError, ResourceOverlay, RouteTerrain};

pub(crate) struct NavalNavigation<'a>(pub &'a GameWorld);

impl RouteTerrain for NavalNavigation<'_> {
    fn route_walkable(
        &self,
        _: &ResourceOverlay,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        self.0
            .unit_tile_passable(UnitKind::FishingShip, tile, cancelled)
    }
    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        Ok(
            self.route_walkable(&ResourceOverlay::default(), from, cancelled)?
                && self.route_walkable(&ResourceOverlay::default(), to, cancelled)?,
        )
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
        if kind != UnitKind::FishingShip {
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
