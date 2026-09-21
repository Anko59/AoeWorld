use aoe_core::{TileCoord, WorldConfig};
use aoe_map::{
    CHUNK_TILES, EdgePassability, ElevationPage, EnvironmentPageProvider, HistoricalLandUsePage,
    MapChunkGenerator, MapPackage, MovementOutcome, PotentialBiomePage, ResourceOverlay,
    RoutePlanner, RoutePlannerPoll, WaterPage, find_path_segment_with_overlay,
    find_path_with_overlay,
};
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

#[path = "terrain_overlay_state.rs"]
mod terrain_overlay_state;

#[path = "start_search.rs"]
mod start_search;
pub use start_search::StartSearchResult;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UniformGrass {
    seed: u64,
}

impl UniformGrass {
    pub const fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub fn material_at(self, tile: TileCoord, config: WorldConfig) -> Option<u8> {
        if tile.x < 0 || tile.y < 0 || tile.x >= config.width_tiles || tile.y >= config.height_tiles
        {
            return None;
        }
        let x = tile.x.rem_euclid(8) as u64;
        let y = tile.y.rem_euclid(8) as u64;
        Some(((x.wrapping_mul(37) + y.wrapping_mul(17) + self.seed) % 8) as u8)
    }
}

#[derive(Debug)]
pub enum Terrain {
    Uniform(UniformGrass),
    Map {
        generator: MapChunkGenerator,
        overlay: ResourceOverlay,
    },
}

impl Terrain {
    pub const fn uniform(seed: u64) -> Self {
        Self::Uniform(UniformGrass::new(seed))
    }

    pub const fn has_map_navigation(&self) -> bool {
        matches!(self, Self::Map { .. })
    }

    pub fn from_package(package: &MapPackage) -> Self {
        Self::Map {
            generator: package.generator(),
            overlay: ResourceOverlay::default(),
        }
    }

    pub fn from_prepared_package(
        package: &MapPackage,
        elevation_pages: Vec<ElevationPage>,
        water_pages: Vec<WaterPage>,
        vegetation_pages: Vec<PotentialBiomePage>,
        land_use_pages: Vec<HistoricalLandUsePage>,
    ) -> Result<Self, aoe_map::MapPackageError> {
        Ok(Self::Map {
            generator: package.generator_with_environment(
                elevation_pages,
                water_pages,
                vegetation_pages,
                land_use_pages,
            )?,
            overlay: ResourceOverlay::default(),
        })
    }

    pub fn from_page_provider(
        package: &MapPackage,
        provider: Arc<dyn EnvironmentPageProvider>,
    ) -> Result<Self, aoe_map::MapPackageError> {
        Ok(Self::Map {
            generator: package.generator_with_page_provider(provider)?,
            overlay: ResourceOverlay::default(),
        })
    }

    pub fn passable(&self, tile: TileCoord, config: WorldConfig) -> bool {
        match self {
            Self::Uniform(_) => {
                tile.x >= 0
                    && tile.y >= 0
                    && tile.x < config.width_tiles
                    && tile.y < config.height_tiles
            }
            Self::Map { .. } => self
                .passable_with_cancel(tile, config, &|| false)
                .unwrap_or(false),
        }
    }

    pub fn passable_with_cancel(
        &self,
        tile: TileCoord,
        config: WorldConfig,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, aoe_map::EnvironmentPageError> {
        match self {
            Self::Uniform(_) => Ok(tile.x >= 0
                && tile.y >= 0
                && tile.x < config.width_tiles
                && tile.y < config.height_tiles),
            Self::Map { generator, overlay } => {
                let Some(sample) = generator.tile_at_with_cancel(tile, cancelled)? else {
                    return Ok(false);
                };
                let object = generator.object_at_with_cancel(tile, cancelled)?;
                Ok(sample.passable && object.is_none_or(|node| !overlay.blocks_node(node)))
            }
        }
    }

    pub fn crossable(&self, from: TileCoord, to: TileCoord, config: WorldConfig) -> bool {
        match self {
            Self::Uniform(_) => self.passable(from, config) && self.passable(to, config),
            Self::Map { .. } => self
                .crossable_with_cancel(from, to, config, &|| false)
                .unwrap_or(false),
        }
    }

    pub fn crossable_with_cancel(
        &self,
        from: TileCoord,
        to: TileCoord,
        config: WorldConfig,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, aoe_map::EnvironmentPageError> {
        match self {
            Self::Uniform(_) => Ok(self.passable(from, config) && self.passable(to, config)),
            Self::Map { generator, overlay } => {
                let step_clear = |from, to| {
                    Ok::<_, aoe_map::EnvironmentPageError>(
                        matches!(
                            generator.edge_between_with_cancel(from, to, cancelled)?,
                            EdgePassability::Passable
                        ) && generator
                            .object_at_with_cancel(to, cancelled)?
                            .is_none_or(|node| !overlay.blocks_node(node)),
                    )
                };
                if !step_clear(from, to)? {
                    return Ok(false);
                }
                let diagonal = from.x != to.x && from.y != to.y;
                Ok(!diagonal
                    || (step_clear(from, TileCoord::new(to.x, from.y))?
                        && step_clear(from, TileCoord::new(from.x, to.y))?))
            }
        }
    }

    pub fn route(&self, origin: TileCoord, destination: TileCoord) -> Option<Vec<TileCoord>> {
        self.route_outcome(origin, destination)
            .map(|outcome| match outcome {
                MovementOutcome::Path(path) => path.tiles,
                MovementOutcome::InvalidDestination
                | MovementOutcome::Unreachable
                | MovementOutcome::BudgetExceeded => Vec::new(),
            })
    }

    /// Returns the authoritative map route result without conflating an
    /// unreachable destination with bounded planner work. Uniform worlds keep
    /// their direct-stepping behavior and return `None`.
    pub fn route_outcome(
        &self,
        origin: TileCoord,
        destination: TileCoord,
    ) -> Option<MovementOutcome> {
        self.route_outcome_with_limit(origin, destination, 4_096)
    }

    pub fn route_outcome_with_limit(
        &self,
        origin: TileCoord,
        destination: TileCoord,
        max_expansions: u32,
    ) -> Option<MovementOutcome> {
        let Self::Map { generator, overlay } = self else {
            return None;
        };
        Some(find_path_with_overlay(
            generator,
            overlay,
            origin,
            destination,
            max_expansions,
        ))
    }

    /// Plans the next fine-scale segment of a map order. Uniform worlds keep
    /// their allocation-free direct stepping and therefore return `None`.
    pub fn route_segment(
        &self,
        origin: TileCoord,
        destination: TileCoord,
    ) -> Option<MovementOutcome> {
        self.route_segment_with_limit(origin, destination, 4_096)
    }

    pub fn route_segment_with_limit(
        &self,
        origin: TileCoord,
        destination: TileCoord,
        max_expansions: u32,
    ) -> Option<MovementOutcome> {
        let Self::Map { generator, overlay } = self else {
            return None;
        };
        Some(find_path_segment_with_overlay(
            generator,
            overlay,
            origin,
            destination,
            max_expansions,
        ))
    }

    pub fn route_planner(
        &self,
        origin: TileCoord,
        destination: TileCoord,
        max_expansions: u32,
    ) -> Option<RoutePlanner> {
        self.has_map_navigation()
            .then(|| RoutePlanner::new(origin, destination, max_expansions))
    }

    pub fn poll_route_planner(
        &self,
        planner: &mut RoutePlanner,
        budget: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<RoutePlannerPoll> {
        let Self::Map { generator, overlay } = self else {
            return None;
        };
        Some(planner.poll(generator, overlay, budget, cancelled))
    }

    /// Counts a connected walkable component without allocating map-scale
    /// state. Callers choose the stopping threshold (for example 256 tiles
    /// for a playable starting area).
    pub fn reachable_tiles(&self, origin: TileCoord, config: WorldConfig, limit: usize) -> usize {
        if limit == 0 || !self.passable(origin, config) {
            return 0;
        }
        let mut visited = BTreeSet::from([origin]);
        let mut pending = VecDeque::from([origin]);
        while let Some(tile) = pending.pop_front() {
            if visited.len() >= limit {
                break;
            }
            for offset_y in -1..=1 {
                for offset_x in -1..=1 {
                    if offset_x == 0 && offset_y == 0 {
                        continue;
                    }
                    let next = TileCoord::new(tile.x + offset_x, tile.y + offset_y);
                    if visited.len() < limit
                        && !visited.contains(&next)
                        && self.crossable(tile, next, config)
                    {
                        visited.insert(next);
                        pending.push_back(next);
                    }
                }
            }
        }
        visited.len()
    }
}

impl Terrain {
    pub(crate) fn chunk_passability_with_cancel(
        &self,
        chunk_x: i32,
        chunk_y: i32,
        config: WorldConfig,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<bool>, aoe_map::EnvironmentPageError> {
        let side = CHUNK_TILES as usize;
        let mut passability = vec![false; side * side];
        let origin = TileCoord::new(chunk_x * CHUNK_TILES, chunk_y * CHUNK_TILES);
        match self {
            Self::Uniform(_) => {
                for local_y in 0..CHUNK_TILES {
                    for local_x in 0..CHUNK_TILES {
                        let tile = TileCoord::new(origin.x + local_x, origin.y + local_y);
                        if tile.x >= 0
                            && tile.y >= 0
                            && tile.x < config.width_tiles
                            && tile.y < config.height_tiles
                        {
                            passability[local_y as usize * side + local_x as usize] = true;
                        }
                    }
                }
            }
            Self::Map { generator, overlay } => {
                let chunk = generator.chunk_with_cancel(chunk_x, chunk_y, cancelled)?;
                let width = (config.width_tiles - origin.x).clamp(0, CHUNK_TILES) as usize;
                if width == 0 {
                    return Ok(passability);
                }
                for (index, sample) in chunk.tiles.iter().enumerate() {
                    let local_y = index / width;
                    let local_x = index % width;
                    if local_y < side && local_x < side {
                        passability[local_y * side + local_x] = sample.passable;
                    }
                }
                for resource in chunk.resources {
                    if overlay.blocks_node(resource) {
                        let local_x = usize::try_from(resource.tile.x - origin.x).unwrap_or(side);
                        let local_y = usize::try_from(resource.tile.y - origin.y).unwrap_or(side);
                        if local_x < side && local_y < side {
                            passability[local_y * side + local_x] = false;
                        }
                    }
                }
            }
        }
        Ok(passability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_core::Seed;

    #[test]
    fn reachable_tiles_stops_at_the_requested_bound() {
        let config = WorldConfig::new(64, 64, Seed(1)).expect("config");
        assert_eq!(
            Terrain::uniform(1).reachable_tiles(TileCoord::new(32, 32), config, 256),
            256
        );
    }

    #[test]
    fn start_selection_uses_center_distance_then_canonical_tile_order() {
        let config = WorldConfig::new(64, 64, Seed(1)).expect("config");
        assert_eq!(
            Terrain::uniform(1).starting_tile(config),
            Some(TileCoord::new(31, 31))
        );
    }
}
