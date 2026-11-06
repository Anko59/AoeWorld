use crate::{GameWorld, UnitKind};
use aoe_core::TileCoord;
use aoe_map::{EnvironmentPageError, ResourceOverlay, RouteTerrain};
use std::cell::RefCell;
use std::collections::HashMap;

/// Water a ship's route search reads during one poll. Each edge test reads
/// up to four tiles (both ends and, for a diagonal, both corners) and every
/// tile is shared by up to eight edges; ground does not change during a
/// poll, so each tile's answer is read once and reused (`RouteTerrain`).
pub(crate) struct NavalNavigation<'a> {
    world: &'a GameWorld,
    wet: RefCell<HashMap<TileCoord, bool>>,
}

impl<'a> NavalNavigation<'a> {
    pub(crate) fn new(world: &'a GameWorld) -> Self {
        Self {
            world,
            wet: RefCell::default(),
        }
    }

    fn wet(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        if let Some(&known) = self.wet.borrow().get(&tile) {
            return Ok(known);
        }
        let wet = !self.world.economy.blocked_tiles.contains(&tile)
            && self
                .world
                .unit_tile_passable(UnitKind::FishingShip, tile, cancelled)?;
        self.wet.borrow_mut().insert(tile, wet);
        Ok(wet)
    }
}

impl RouteTerrain for NavalNavigation<'_> {
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
        let wet = |tile| self.wet(tile, cancelled);
        Ok(wet(from)?
            && wet(to)?
            && (from.x == to.x
                || from.y == to.y
                || (wet(TileCoord::new(from.x, to.y))? && wet(TileCoord::new(to.x, from.y))?)))
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
