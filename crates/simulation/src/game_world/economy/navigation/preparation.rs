//! One deterministic preparation allowance shared by every planner in a tick.
use aoe_core::TileCoord;
use aoe_map::{EnvironmentPageError, MapChunkGenerator, ResourceOverlay, RouteTerrain};
use std::sync::Mutex;

pub(super) struct LimitedMap<'a> {
    pub(super) generator: &'a MapChunkGenerator,
    pub(super) allowance: &'a Mutex<u32>,
}
impl RouteTerrain for LimitedMap<'_> {
    fn route_generator(&self) -> Option<&MapChunkGenerator> {
        Some(self.generator)
    }
    fn prepare_navigation_chunk(&self) -> bool {
        let mut left = self
            .allowance
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if *left == 0 {
            false
        } else {
            *left -= 1;
            true
        }
    }
    fn route_walkable(
        &self,
        overlay: &ResourceOverlay,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        self.generator.route_walkable(overlay, tile, cancelled)
    }
    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        self.generator.route_edge_clear(from, to, cancelled)
    }
    fn route_step_cost(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<u32, EnvironmentPageError> {
        self.generator.route_step_cost(from, to, cancelled)
    }
}
