//! One deterministic preparation allowance shared by every planner in a tick.
use aoe_core::TileCoord;
use aoe_map::{EnvironmentPageError, MapChunkGenerator, ResourceOverlay, RouteTerrain};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct LimitedMap<'a> {
    pub(super) generator: &'a MapChunkGenerator,
    pub(super) allowance: &'a Mutex<u32>,
    /// Set when a chunk preparation was refused for want of allowance.
    pub(super) denied: Option<&'a AtomicBool>,
    pub(super) world: &'a crate::GameWorld,
    pub(super) player: aoe_core::PlayerId,
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
            if let Some(denied) = self.denied {
                denied.store(true, Ordering::Relaxed);
            }
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
        if !self.world.gate_tile_admitted(self.player, tile) {
            return Ok(false);
        }
        // On the live overlay this is movement's land passability, which the
        // world keeps per tile across ticks; a proof snapshot reads directly.
        match &self.world.terrain {
            crate::Terrain::Map { overlay: live, .. } if std::ptr::eq(overlay, live) => self
                .world
                .unit_tile_passable(crate::UnitKind::Villager, tile, cancelled),
            _ => self.generator.route_walkable(overlay, tile, cancelled),
        }
    }
    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        Ok(self.world.gate_tile_admitted(self.player, from)
            && self.world.gate_tile_admitted(self.player, to)
            && self.generator.route_edge_clear(from, to, cancelled)?)
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
