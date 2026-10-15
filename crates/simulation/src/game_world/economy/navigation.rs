//! Uniform villages feed occupancy into the same retained route planner as maps.
use crate::{GameWorld, Terrain};
use aoe_core::{TileCoord, WorldConfig};
use aoe_map::{
    EnvironmentPageError, ResourceOverlay, RoutePlanner, RoutePlannerPoll, RouteTerrain,
};
use std::collections::BTreeSet;

struct UniformNavigation<'a> {
    config: WorldConfig,
    blocked: &'a BTreeSet<TileCoord>,
}

impl UniformNavigation<'_> {
    fn passable(&self, tile: TileCoord) -> bool {
        tile.x >= 0
            && tile.y >= 0
            && tile.x < self.config.width_tiles
            && tile.y < self.config.height_tiles
            && !self.blocked.contains(&tile)
    }
}

impl RouteTerrain for UniformNavigation<'_> {
    fn route_walkable(
        &self,
        _: &ResourceOverlay,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        Ok(self.passable(tile))
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
        Ok(self.passable(from) && self.passable(to))
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
    pub(crate) fn has_route_navigation(&self) -> bool {
        self.terrain.has_map_navigation() || !self.economy.blocked_tiles.is_empty()
    }

    pub(crate) fn poll_world_route_planner(
        &self,
        planner: &mut RoutePlanner,
        budget: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> RoutePlannerPoll {
        match &self.terrain {
            Terrain::Map { generator, overlay } => {
                planner.poll(generator, overlay, budget, cancelled)
            }
            Terrain::Uniform(_) => planner.poll(
                &UniformNavigation {
                    config: self.config,
                    blocked: &self.economy.blocked_tiles,
                },
                &ResourceOverlay::default(),
                budget,
                cancelled,
            ),
        }
    }

    pub(crate) fn plan_world_route_ahead(
        &self,
        planner: &mut RoutePlanner,
        budget: u32,
        cancelled: &dyn Fn() -> bool,
    ) {
        match &self.terrain {
            Terrain::Map { generator, overlay } => {
                planner.plan_ahead(generator, overlay, budget, cancelled)
            }
            Terrain::Uniform(_) => planner.plan_ahead(
                &UniformNavigation {
                    config: self.config,
                    blocked: &self.economy.blocked_tiles,
                },
                &ResourceOverlay::default(),
                budget,
                cancelled,
            ),
        }
    }
}
