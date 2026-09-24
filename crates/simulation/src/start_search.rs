use super::Terrain;
use aoe_core::{TileCoord, WorldConfig};
use aoe_map::{CHUNK_TILES, EnvironmentPageError};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const START_CLEAR_RADIUS: i32 = 2;
const START_REACHABLE_TILES: usize = 256;
const START_SEARCH_CHUNKS: usize = 64;
const START_CACHE_CHUNKS: usize = 256;
const LEGACY_START_RECIPE: u16 = 3;
const RECIPE_4_START_RECIPE: u16 = 4;
const RECIPE_5_START_RECIPE: u16 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartSearchResult {
    Found(TileCoord),
    Unavailable,
    LimitReached,
    Cancelled,
}

impl Terrain {
    /// Finds the closest playable 5×5 clearing without allocating terrain
    /// state proportional to the virtual map area. Candidates use squared
    /// tile-center distance, followed by canonical `(y, x)` ordering.
    pub fn starting_tile(&self, config: WorldConfig) -> Option<TileCoord> {
        match self.search_start(config, START_SEARCH_CHUNKS, || false) {
            StartSearchResult::Found(tile) => Some(tile),
            _ => None,
        }
    }

    /// Searches a bounded number of chunks. Cancellation is supplied by the
    /// adapter; pure terrain does not read clocks, files, or process state.
    pub fn search_start(
        &self,
        config: WorldConfig,
        max_chunks: usize,
        cancelled: impl Fn() -> bool,
    ) -> StartSearchResult {
        self.search_start_checked(config, max_chunks, cancelled)
            .unwrap_or(StartSearchResult::Unavailable)
    }

    pub fn search_start_checked(
        &self,
        config: WorldConfig,
        max_chunks: usize,
        cancelled: impl Fn() -> bool,
    ) -> Result<StartSearchResult, EnvironmentPageError> {
        self.search_start_for_recipe(config, LEGACY_START_RECIPE, max_chunks, cancelled)
    }

    /// Validates the package recipe while preserving the established start
    /// footprint and bounded search for all supported generation recipes.
    pub fn search_start_for_recipe(
        &self,
        config: WorldConfig,
        generation_recipe_version: u16,
        max_chunks: usize,
        cancelled: impl Fn() -> bool,
    ) -> Result<StartSearchResult, EnvironmentPageError> {
        match generation_recipe_version {
            LEGACY_START_RECIPE | RECIPE_4_START_RECIPE | RECIPE_5_START_RECIPE => {}
            _ => return Err(EnvironmentPageError::Invalid),
        }
        if cancelled() {
            return Ok(StartSearchResult::Cancelled);
        }
        if max_chunks == 0 {
            return Ok(StartSearchResult::LimitReached);
        }
        let center = TileCoord::new((config.width_tiles - 1) / 2, (config.height_tiles - 1) / 2);
        let mut cache = StartPassabilityCache::new(self, config, &cancelled);
        if valid_start(&mut cache, center)? {
            return Ok(StartSearchResult::Found(center));
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
        let mut scanned = 0;
        for ring in 0..=max_ring {
            for (chunk_x, chunk_y) in ring_chunks(center_chunk, ring) {
                if chunk_x < 0 || chunk_y < 0 || chunk_x >= chunks_x || chunk_y >= chunks_y {
                    continue;
                }
                if cancelled() {
                    return Ok(StartSearchResult::Cancelled);
                }
                if scanned == max_chunks {
                    return Ok(StartSearchResult::LimitReached);
                }
                scanned += 1;
                scan_start_chunk(&mut cache, chunk_x, chunk_y, &mut best)?;
            }
            if best.is_some_and(|tile| farther_than_best(tile, config, center_chunk, ring)) {
                return Ok(best.map_or(StartSearchResult::Unavailable, StartSearchResult::Found));
            }
        }
        Ok(best.map_or(StartSearchResult::Unavailable, StartSearchResult::Found))
    }
}

struct StartPassabilityCache<'a> {
    terrain: &'a Terrain,
    config: WorldConfig,
    entries: BTreeMap<(i32, i32), Vec<bool>>,
    insertion_order: VecDeque<(i32, i32)>,
    reachable: BTreeMap<TileCoord, bool>,
    cancelled: &'a dyn Fn() -> bool,
}

impl<'a> StartPassabilityCache<'a> {
    fn new(terrain: &'a Terrain, config: WorldConfig, cancelled: &'a dyn Fn() -> bool) -> Self {
        Self {
            terrain,
            config,
            entries: BTreeMap::new(),
            insertion_order: VecDeque::new(),
            reachable: BTreeMap::new(),
            cancelled,
        }
    }

    fn reaches_required_tiles(&mut self, origin: TileCoord) -> Result<bool, EnvironmentPageError> {
        if let Some(result) = self.reachable.get(&origin) {
            return Ok(*result);
        }
        let mut visited = BTreeSet::from([origin]);
        let mut pending = VecDeque::from([origin]);
        let mut result = false;
        while let Some(tile) = pending.pop_front() {
            if visited.len() >= START_REACHABLE_TILES {
                result = true;
                break;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let next = TileCoord::new(tile.x + dx, tile.y + dy);
                    if visited.contains(&next) || !self.passable(next)? {
                        continue;
                    }
                    if !self.terrain.crossable_with_cancel(
                        tile,
                        next,
                        self.config,
                        self.cancelled,
                    )? {
                        continue;
                    }
                    visited.insert(next);
                    pending.push_back(next);
                }
            }
        }
        // Retain a bounded memo: small disconnected components are explored
        // once, rather than for every possible clearing inside them.
        if self.reachable.len() + visited.len() > START_CACHE_CHUNKS * 1_024 {
            self.reachable.clear();
        }
        if result {
            self.reachable.insert(origin, true);
        } else {
            for tile in visited {
                self.reachable.insert(tile, false);
            }
        }
        Ok(result)
    }

    fn passable(&mut self, tile: TileCoord) -> Result<bool, EnvironmentPageError> {
        if tile.x < 0
            || tile.y < 0
            || tile.x >= self.config.width_tiles
            || tile.y >= self.config.height_tiles
        {
            return Ok(false);
        }
        if matches!(self.terrain, Terrain::Uniform(_)) {
            return Ok(true);
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
            let passability = self.terrain.chunk_passability_with_cancel(
                chunk_x,
                chunk_y,
                self.config,
                self.cancelled,
            )?;
            self.entries.insert(key, passability);
            self.insertion_order.push_back(key);
        }
        let local_x = usize::try_from(tile.x.rem_euclid(CHUNK_TILES)).unwrap_or(0);
        let local_y = usize::try_from(tile.y.rem_euclid(CHUNK_TILES)).unwrap_or(0);
        Ok(self
            .entries
            .get(&key)
            .and_then(|tiles| tiles.get(local_y * CHUNK_TILES as usize + local_x))
            .copied()
            .unwrap_or(false))
    }
}

fn scan_start_chunk(
    cache: &mut StartPassabilityCache<'_>,
    chunk_x: i32,
    chunk_y: i32,
    best: &mut Option<TileCoord>,
) -> Result<(), EnvironmentPageError> {
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
        if valid_start(cache, candidate)? {
            *best = Some(candidate);
        }
    }
    Ok(())
}

fn valid_start(
    cache: &mut StartPassabilityCache<'_>,
    candidate: TileCoord,
) -> Result<bool, EnvironmentPageError> {
    Ok(clear_starting_area(cache, candidate)? && cache.reaches_required_tiles(candidate)?)
}

fn clear_starting_area(
    cache: &mut StartPassabilityCache<'_>,
    center: TileCoord,
) -> Result<bool, EnvironmentPageError> {
    for offset_y in -START_CLEAR_RADIUS..=START_CLEAR_RADIUS {
        for offset_x in -START_CLEAR_RADIUS..=START_CLEAR_RADIUS {
            if !cache.passable(TileCoord::new(center.x + offset_x, center.y + offset_y))? {
                return Ok(false);
            }
        }
    }
    Ok(true)
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

fn ring_chunks(center: TileCoord, ring: i32) -> Vec<(i32, i32)> {
    if ring == 0 {
        return vec![(center.x, center.y)];
    }
    let left = center.x - ring;
    let right = center.x + ring;
    let top = center.y - ring;
    let bottom = center.y + ring;
    let mut chunks = Vec::with_capacity((ring as usize) * 8);
    for x in left..=right {
        chunks.push((x, top));
    }
    for y in top + 1..bottom {
        chunks.extend([(left, y), (right, y)]);
    }
    for x in left..=right {
        chunks.push((x, bottom));
    }
    chunks
}

#[cfg(test)]
#[path = "start_search/tests/start_search.rs"]
mod tests;
