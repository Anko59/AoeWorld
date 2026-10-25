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
    /// Attach an adapter-owned profiler without changing authoritative state.
    pub fn set_planning_probe(&mut self, probe: std::sync::Arc<dyn crate::PlanningProbe>) {
        self.planning_probe = Some(probe);
    }

    fn measure_planning(
        &self,
    ) -> Option<crate::game_world::route_scheduler::PlanningMeasurement<'_>> {
        self.planning_probe.as_ref().map(|probe| {
            probe.begin();
            crate::game_world::route_scheduler::PlanningMeasurement(probe.as_ref())
        })
    }

    /// Prepare the immutable map hierarchy before starting the tick clock.
    pub fn prepare_navigation(
        &self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), EnvironmentPageError> {
        if let Terrain::Map { generator, overlay } = &self.terrain {
            generator.prepare_navigation(overlay, cancelled)?;
        }
        Ok(())
    }

    pub(crate) fn has_route_navigation(&self) -> bool {
        self.terrain.has_map_navigation() || !self.economy.blocked_tiles.is_empty()
    }

    pub(crate) fn poll_world_route_planner(
        &self,
        planner: &mut RoutePlanner,
        budget: u32,
        cancelled: &dyn Fn() -> bool,
        kind: crate::UnitKind,
    ) -> RoutePlannerPoll {
        let _measurement = self.measure_planning();
        if kind == crate::UnitKind::FishingShip {
            return planner.poll(
                &super::fishing::navigation::NavalNavigation(self),
                &ResourceOverlay::default(),
                budget,
                cancelled,
            );
        }
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
        kind: crate::UnitKind,
    ) {
        let _measurement = self.measure_planning();
        if kind == crate::UnitKind::FishingShip {
            planner.plan_ahead(
                &super::fishing::navigation::NavalNavigation(self),
                &ResourceOverlay::default(),
                budget,
                cancelled,
            );
            return;
        }
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
