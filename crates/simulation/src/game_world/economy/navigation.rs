//! Uniform villages feed occupancy into the same retained route planner as maps.
use crate::{GameWorld, Terrain};
use aoe_core::{TileCoord, WorldConfig};
use aoe_map::{
    EnvironmentPageError, ResourceOverlay, RoutePlanner, RoutePlannerPoll, RouteTerrain,
};

#[path = "navigation/preparation.rs"]
mod preparation;

struct UniformNavigation<'a> {
    config: WorldConfig,
    player: aoe_core::PlayerId,
    world: &'a GameWorld,
}

impl UniformNavigation<'_> {
    fn passable(&self, tile: TileCoord) -> bool {
        tile.x >= 0
            && tile.y >= 0
            && tile.x < self.config.width_tiles
            && tile.y < self.config.height_tiles
            && self.world.route_tile_admitted(self.player, tile)
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
    /// A closed completed gate is a legal destination for its owner: approaching
    /// opens it. Collision remains closed until that approach actually happens.
    pub(crate) fn gate_allows_approach(&self, player: aoe_core::PlayerId, tile: TileCoord) -> bool {
        self.building_at_tile(tile)
            .and_then(|id| self.building(id))
            .is_some_and(|b| {
                b.kind.is_gate()
                    && b.is_complete()
                    && !b.gate_locked
                    && (b.player == player || b.gate_open)
            })
    }

    /// Bounded, rotating immutable cache hints for an adapter-owned worker.
    /// Reading hints never changes authoritative scheduling or budgets.
    pub fn navigation_preparation_tiles(&self, limit: usize) -> Vec<TileCoord> {
        let mut tiles = Vec::new();
        let count = self.active_movers.len();
        if count == 0 {
            return tiles;
        }
        let start = (self.tick.0 % count as u64) as usize;
        let rotated = self.active_movers.iter().skip(start);
        for id in rotated
            .chain(self.active_movers.iter())
            .take(count.min(limit))
        {
            let Some(&index) = self.lookup.get(&id) else {
                continue;
            };
            let unit = &self.units[index];
            tiles.push(unit.state.position.tile_floor());
            tiles.extend(
                unit.route
                    .iter()
                    .take(2)
                    .copied()
                    .map(aoe_core::WorldPosition::tile_floor),
            );
            if let Some(planner) = &unit.planner {
                tiles.extend(planner.preparation_tiles());
            }
            if tiles.len() >= limit {
                tiles.truncate(limit);
                break;
            }
        }
        tiles
    }

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

    /// Player-specific planning admission; a locked gate blocks even while
    /// physically held open. Other buildings retain their collision footprint.
    pub(crate) fn route_tile_admitted(&self, player: aoe_core::PlayerId, tile: TileCoord) -> bool {
        if self
            .building_at_tile(tile)
            .and_then(|id| self.building(id))
            .is_some_and(|b| b.kind.is_gate())
        {
            return self.gate_allows_approach(player, tile);
        }
        !self.economy.blocked_tiles.contains(&tile)
    }

    pub(crate) fn has_route_navigation(&self) -> bool {
        self.terrain.has_map_navigation()
            || !self.economy.blocked_tiles.is_empty()
            || !self.economy.gates.is_empty()
    }

    pub(crate) fn route_origin_pocket(
        &self,
        kind: crate::UnitKind,
        origin: TileCoord,
        player: aoe_core::PlayerId,
    ) -> Result<Option<aoe_map::RouteOriginPocket>, EnvironmentPageError> {
        if kind.is_naval() {
            return aoe_map::RouteOriginPocket::build(
                &super::fishing::navigation::NavalNavigation::new(self),
                ResourceOverlay::empty(),
                origin,
                &|| false,
            );
        }
        match &self.terrain {
            Terrain::Map { generator, overlay } => aoe_map::RouteOriginPocket::build(
                &preparation::LimitedMap {
                    generator,
                    allowance: &self.navigation_preparation_budget,
                    world: self,
                    player,
                },
                overlay,
                origin,
                &|| false,
            ),
            Terrain::Uniform(_) => aoe_map::RouteOriginPocket::build(
                &UniformNavigation {
                    config: self.config,
                    player,
                    world: self,
                },
                ResourceOverlay::empty(),
                origin,
                &|| false,
            ),
        }
    }

    pub(crate) fn poll_world_route_planner(
        &self,
        planner: &mut RoutePlanner,
        budget: u32,
        cancelled: &dyn Fn() -> bool,
        kind: crate::UnitKind,
        player: aoe_core::PlayerId,
    ) -> RoutePlannerPoll {
        let _measurement = self.measure_planning();
        if kind.is_naval() {
            return planner.poll(
                &super::fishing::navigation::NavalNavigation::new(self),
                ResourceOverlay::empty(),
                budget,
                cancelled,
            );
        }
        match &self.terrain {
            Terrain::Map { generator, overlay } => planner.poll(
                &preparation::LimitedMap {
                    generator,
                    allowance: &self.navigation_preparation_budget,
                    world: self,
                    player,
                },
                overlay,
                budget,
                cancelled,
            ),
            Terrain::Uniform(_) => planner.poll(
                &UniformNavigation {
                    config: self.config,
                    player,
                    world: self,
                },
                ResourceOverlay::empty(),
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
        player: aoe_core::PlayerId,
    ) {
        let _measurement = self.measure_planning();
        if kind.is_naval() {
            planner.plan_ahead(
                &super::fishing::navigation::NavalNavigation::new(self),
                ResourceOverlay::empty(),
                budget,
                cancelled,
            );
            return;
        }
        match &self.terrain {
            Terrain::Map { generator, overlay } => planner.plan_ahead(
                &preparation::LimitedMap {
                    generator,
                    allowance: &self.navigation_preparation_budget,
                    world: self,
                    player,
                },
                overlay,
                budget,
                cancelled,
            ),
            Terrain::Uniform(_) => planner.plan_ahead(
                &UniformNavigation {
                    config: self.config,
                    player,
                    world: self,
                },
                ResourceOverlay::empty(),
                budget,
                cancelled,
            ),
        }
    }
    /// Polls `planner` with `budget` work. Spends no tick budget, preparation
    /// allowance or counter, so a probe never changes the simulation.
    pub fn probe_route_poll(
        &self,
        planner: &mut RoutePlanner,
        budget: u32,
        kind: crate::UnitKind,
        player: aoe_core::PlayerId,
    ) -> RoutePlannerPoll {
        if let Terrain::Map { generator, overlay } = &self.terrain
            && !kind.is_naval()
        {
            let allowance = std::sync::Mutex::new(u32::MAX);
            planner.poll(
                &preparation::LimitedMap {
                    generator,
                    allowance: &allowance,
                    world: self,
                    player,
                },
                overlay,
                budget,
                &|| false,
            )
        } else {
            self.poll_world_route_planner(planner, budget, &|| false, kind, player)
        }
    }
}
