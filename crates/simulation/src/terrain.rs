use aoe_core::{TileCoord, WorldConfig};
use aoe_map::{
    CHUNK_TILES, Depletion, EdgePassability, ElevationPage, HistoricalLandUsePage,
    MapChunkGenerator, MapPackage, MovementOutcome, PotentialBiomePage, ResourceOverlay,
    ResourceOverlayError, WaterPage, find_path_segment_with_overlay, find_path_with_overlay,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const START_CLEAR_RADIUS: i32 = 2;
const START_REACHABLE_TILES: usize = 256;
const START_CACHE_CHUNKS: usize = 256;

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

    pub fn passable(&self, tile: TileCoord, config: WorldConfig) -> bool {
        match self {
            Self::Uniform(_) => {
                tile.x >= 0
                    && tile.y >= 0
                    && tile.x < config.width_tiles
                    && tile.y < config.height_tiles
            }
            Self::Map { generator, overlay } => generator.tile_at(tile).is_some_and(|sample| {
                sample.passable
                    && generator
                        .object_at(tile)
                        .is_none_or(|node| !overlay.blocks(generator, node.id))
            }),
        }
    }

    pub fn crossable(&self, from: TileCoord, to: TileCoord, config: WorldConfig) -> bool {
        match self {
            Self::Uniform(_) => self.passable(from, config) && self.passable(to, config),
            Self::Map { generator, overlay } => map_crossable(generator, overlay, from, to),
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

    /// Applies a deterministic resource depletion to map terrain. Exhausted
    /// resources immediately stop blocking passability through the overlay.
    pub fn deplete_resource(
        &mut self,
        id: u64,
        requested: u16,
    ) -> Result<Depletion, ResourceOverlayError> {
        let Self::Map { generator, overlay } = self else {
            return Err(ResourceOverlayError::UnknownResource);
        };
        overlay.deplete(generator, id, requested)
    }

    /// Finds the closest playable 5×5 clearing without allocating terrain
    /// state proportional to the virtual map area. Candidates use squared
    /// tile-center distance, followed by canonical `(y, x)` ordering.
    pub fn starting_tile(&self, config: WorldConfig) -> Option<TileCoord> {
        let center = TileCoord::new((config.width_tiles - 1) / 2, (config.height_tiles - 1) / 2);
        let mut cache = StartPassabilityCache::new(self, config);
        if valid_start(self, &mut cache, center) {
            return Some(center);
        }
        let center_chunk = TileCoord::new(
            center.x.div_euclid(CHUNK_TILES),
            center.y.div_euclid(CHUNK_TILES),
        );
        let chunks_x = (config.width_tiles + CHUNK_TILES - 1) / CHUNK_TILES;
        let chunks_y = (config.height_tiles + CHUNK_TILES - 1) / CHUNK_TILES;
        let max_ring = center_chunk
            .x
            .max(chunks_x - 1 - center_chunk.x)
            .max(center_chunk.y)
            .max(chunks_y - 1 - center_chunk.y);
        let mut best = None;
        for ring in 0..=max_ring {
            for chunk_y in center_chunk.y - ring..=center_chunk.y + ring {
                for chunk_x in center_chunk.x - ring..=center_chunk.x + ring {
                    if chunk_x < 0
                        || chunk_y < 0
                        || chunk_x >= chunks_x
                        || chunk_y >= chunks_y
                        || (chunk_x - center_chunk.x)
                            .unsigned_abs()
                            .max((chunk_y - center_chunk.y).unsigned_abs())
                            != ring as u32
                    {
                        continue;
                    }
                    scan_start_chunk(self, &mut cache, chunk_x, chunk_y, &mut best);
                }
            }
            if best.is_some_and(|tile| farther_than_best(tile, config, center_chunk, ring)) {
                return best;
            }
        }
        best
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

struct StartPassabilityCache<'a> {
    terrain: &'a Terrain,
    config: WorldConfig,
    entries: BTreeMap<(i32, i32), Vec<bool>>,
    insertion_order: VecDeque<(i32, i32)>,
}

impl<'a> StartPassabilityCache<'a> {
    fn new(terrain: &'a Terrain, config: WorldConfig) -> Self {
        Self {
            terrain,
            config,
            entries: BTreeMap::new(),
            insertion_order: VecDeque::new(),
        }
    }

    fn passable(&mut self, tile: TileCoord) -> bool {
        if tile.x < 0
            || tile.y < 0
            || tile.x >= self.config.width_tiles
            || tile.y >= self.config.height_tiles
        {
            return false;
        }
        if matches!(self.terrain, Terrain::Uniform(_)) {
            return true;
        }
        let chunk_x = tile.x.div_euclid(CHUNK_TILES);
        let chunk_y = tile.y.div_euclid(CHUNK_TILES);
        let key = (chunk_x, chunk_y);
        if !self.entries.contains_key(&key) {
            if self.entries.len() == START_CACHE_CHUNKS
                && let Some(expired) = self.insertion_order.pop_front()
            {
                self.entries.remove(&expired);
            }
            self.entries.insert(
                key,
                self.terrain
                    .chunk_passability(chunk_x, chunk_y, self.config),
            );
            self.insertion_order.push_back(key);
        }
        let local_x = usize::try_from(tile.x.rem_euclid(CHUNK_TILES)).unwrap_or(0);
        let local_y = usize::try_from(tile.y.rem_euclid(CHUNK_TILES)).unwrap_or(0);
        self.entries
            .get(&key)
            .and_then(|tiles| tiles.get(local_y * CHUNK_TILES as usize + local_x))
            .copied()
            .unwrap_or(false)
    }
}

impl Terrain {
    fn chunk_passability(&self, chunk_x: i32, chunk_y: i32, config: WorldConfig) -> Vec<bool> {
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
                let chunk = generator.chunk(chunk_x, chunk_y);
                let width = (config.width_tiles - origin.x).clamp(0, CHUNK_TILES) as usize;
                if width == 0 {
                    return passability;
                }
                for (index, sample) in chunk.tiles.iter().enumerate() {
                    let local_y = index / width;
                    let local_x = index % width;
                    if local_y < side && local_x < side {
                        passability[local_y * side + local_x] = sample.passable;
                    }
                }
                for resource in chunk.resources {
                    if overlay.blocks(generator, resource.id) {
                        let local_x = usize::try_from(resource.tile.x - origin.x).unwrap_or(side);
                        let local_y = usize::try_from(resource.tile.y - origin.y).unwrap_or(side);
                        if local_x < side && local_y < side {
                            passability[local_y * side + local_x] = false;
                        }
                    }
                }
            }
        }
        passability
    }
}

fn scan_start_chunk(
    terrain: &Terrain,
    cache: &mut StartPassabilityCache<'_>,
    chunk_x: i32,
    chunk_y: i32,
    best: &mut Option<TileCoord>,
) {
    let config = cache.config;
    let origin = TileCoord::new(chunk_x * CHUNK_TILES, chunk_y * CHUNK_TILES);
    let mut candidates = (origin.y..(origin.y + CHUNK_TILES).min(config.height_tiles))
        .flat_map(|y| {
            (origin.x..(origin.x + CHUNK_TILES).min(config.width_tiles))
                .map(move |x| TileCoord::new(x, y))
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable_by_key(|tile| start_key(*tile, config));
    for candidate in candidates {
        if best.is_some_and(|current| start_key(candidate, config) >= start_key(current, config)) {
            continue;
        }
        if valid_start(terrain, cache, candidate) {
            *best = Some(candidate);
        }
    }
}

fn valid_start(
    terrain: &Terrain,
    cache: &mut StartPassabilityCache<'_>,
    candidate: TileCoord,
) -> bool {
    clear_starting_area(cache, candidate)
        && terrain.reachable_tiles(candidate, cache.config, START_REACHABLE_TILES)
            >= START_REACHABLE_TILES
}

fn clear_starting_area(cache: &mut StartPassabilityCache<'_>, center: TileCoord) -> bool {
    (-START_CLEAR_RADIUS..=START_CLEAR_RADIUS).all(|offset_y| {
        (-START_CLEAR_RADIUS..=START_CLEAR_RADIUS).all(|offset_x| {
            cache.passable(TileCoord::new(center.x + offset_x, center.y + offset_y))
        })
    })
}

fn farther_than_best(
    best: TileCoord,
    config: WorldConfig,
    center_chunk: TileCoord,
    ring: i32,
) -> bool {
    let min_x = (center_chunk.x - ring).max(0) * CHUNK_TILES;
    let max_x = ((center_chunk.x + ring + 1) * CHUNK_TILES - 1).min(config.width_tiles - 1);
    let min_y = (center_chunk.y - ring).max(0) * CHUNK_TILES;
    let max_y = ((center_chunk.y + ring + 1) * CHUNK_TILES - 1).min(config.height_tiles - 1);
    let edge_distance = [
        min_x
            .checked_sub(1)
            .map(|x| centered_distance_squared(x, config.width_tiles)),
        max_x
            .checked_add(1)
            .filter(|x| *x < config.width_tiles)
            .map(|x| centered_distance_squared(x, config.width_tiles)),
        min_y
            .checked_sub(1)
            .map(|y| centered_distance_squared(y, config.height_tiles)),
        max_y
            .checked_add(1)
            .filter(|y| *y < config.height_tiles)
            .map(|y| centered_distance_squared(y, config.height_tiles)),
    ]
    .into_iter()
    .flatten()
    .min()
    .unwrap_or(u64::MAX);
    start_key(best, config).0 < edge_distance
}

fn start_key(tile: TileCoord, config: WorldConfig) -> (u64, i32, i32) {
    (
        centered_distance_squared(tile.x, config.width_tiles)
            .saturating_add(centered_distance_squared(tile.y, config.height_tiles)),
        tile.y,
        tile.x,
    )
}

fn centered_distance_squared(coordinate: i32, length: i32) -> u64 {
    let doubled = i64::from(coordinate) * 2 - i64::from(length - 1);
    doubled.unsigned_abs().pow(2)
}

fn map_crossable(
    generator: &MapChunkGenerator,
    overlay: &ResourceOverlay,
    from: TileCoord,
    to: TileCoord,
) -> bool {
    let step_clear = |from, to| {
        matches!(generator.edge_between(from, to), EdgePassability::Passable)
            && generator
                .object_at(to)
                .is_none_or(|node| !overlay.blocks(generator, node.id))
    };
    if !step_clear(from, to) {
        return false;
    }
    let diagonal = from.x != to.x && from.y != to.y;
    !diagonal
        || (step_clear(from, TileCoord::new(to.x, from.y))
            && step_clear(from, TileCoord::new(from.x, to.y)))
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
