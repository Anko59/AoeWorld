use super::Terrain;
use aoe_core::{TileCoord, WorldConfig};
use aoe_map::{CHUNK_TILES, EnvironmentPageError};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const START_CLEAR_RADIUS: i32 = 2;
const START_REACHABLE_TILES: usize = 256;
const START_SEARCH_CHUNKS: usize = 64;
const START_CACHE_CHUNKS: usize = 256;
const START_EXIT_DISTANCE: u32 = 64;
const START_EXIT_VISITS: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartSearchResult {
    Found(TileCoord),
    Unavailable,
    LimitReached,
    Cancelled,
}

impl Terrain {
    /// Finds a certified 5×5 clearing, preferring the largest surveyed land
    /// region, then centre distance and canonical `(y, x)` ordering. Survey
    /// storage is bounded independently of the virtual map area.
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

    /// A start is a clear 5×5 footprint whose reachable area includes a
    /// bounded exit; exhausting the exit budget reports `LimitReached`.
    pub fn search_start_checked(
        &self,
        config: WorldConfig,
        max_chunks: usize,
        cancelled: impl Fn() -> bool,
    ) -> Result<StartSearchResult, EnvironmentPageError> {
        if cancelled() {
            return Ok(StartSearchResult::Cancelled);
        }
        if max_chunks == 0 {
            return Ok(StartSearchResult::LimitReached);
        }
        let center = TileCoord::new((config.width_tiles - 1) / 2, (config.height_tiles - 1) / 2);
        let mut cache = StartPassabilityCache::new(self, config, &cancelled);
        // Uniform ground has one region and needs no survey.
        if matches!(self, Terrain::Uniform(_)) {
            let valid = valid_start(&mut cache, center)?;
            return Ok(if cancelled() {
                StartSearchResult::Cancelled
            } else if valid {
                StartSearchResult::Found(center)
            } else {
                StartSearchResult::Unavailable
            });
        }
        let (candidates, complete) = regions::candidates(&mut cache, max_chunks)?;
        certify_regions(&mut cache, candidates, complete)
    }
}

struct StartPassabilityCache<'a> {
    terrain: &'a Terrain,
    config: WorldConfig,
    entries: BTreeMap<(i32, i32), Vec<bool>>,
    insertion_order: VecDeque<(i32, i32)>,
    reachable: BTreeMap<TileCoord, bool>,
    exit_visits: usize,
    exit_visit_limit: usize,
    exit_chunk_limit: usize,
    exit_chunks: BTreeSet<(i32, i32)>,
    exit_budget_exhausted: bool,
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
            exit_visits: 0,
            exit_visit_limit: START_EXIT_VISITS,
            exit_chunk_limit: START_SEARCH_CHUNKS,
            exit_chunks: BTreeSet::new(),
            exit_budget_exhausted: false,
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

    /// Every start certifies an actual route beyond any starting glade.
    /// A distance-prioritized frontier avoids exploring an entire open disk.
    /// Work and chunk bounds apply across ALL candidates in this search.
    fn reaches_exit(&mut self, origin: TileCoord) -> Result<bool, EnvironmentPageError> {
        use std::collections::BinaryHeap;
        if self.exit_visits >= self.exit_visit_limit {
            self.exit_budget_exhausted = true;
            return Ok(false);
        }
        self.exit_visits += 1;
        let mut visited = BTreeSet::from([origin]);
        let mut pending = BinaryHeap::from([(0_u32, std::cmp::Reverse((origin.y, origin.x)))]);
        // Large maps still require 64 tiles, beyond the central glade's 27.
        // Small maps certify a route toward their edge rather than requiring
        // a destination outside the world. Keep the work bounds unchanged.
        let exit_distance = START_EXIT_DISTANCE
            .min(((self.config.width_tiles.max(self.config.height_tiles) - 1) / 2).max(1) as u32);
        while let Some((distance, std::cmp::Reverse((y, x)))) = pending.pop() {
            if (self.cancelled)() {
                return Ok(false);
            }
            if distance >= exit_distance {
                return Ok(true);
            }
            let tile = TileCoord::new(x, y);
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let next = TileCoord::new(x + dx, y + dy);
                if visited.contains(&next) {
                    continue;
                }
                let chunk = (
                    next.x.div_euclid(CHUNK_TILES),
                    next.y.div_euclid(CHUNK_TILES),
                );
                if !self.exit_chunks.contains(&chunk)
                    && self.exit_chunks.len() >= self.exit_chunk_limit
                {
                    self.exit_budget_exhausted = true;
                    return Ok(false);
                }
                self.exit_chunks.insert(chunk);
                if !self.passable(next)? {
                    continue;
                }
                if self
                    .terrain
                    .crossable_with_cancel(tile, next, self.config, self.cancelled)?
                {
                    if self.exit_visits >= self.exit_visit_limit {
                        self.exit_budget_exhausted = true;
                        return Ok(false);
                    }
                    self.exit_visits += 1;
                    visited.insert(next);
                    pending.push((
                        next.x.abs_diff(origin.x).max(next.y.abs_diff(origin.y)),
                        std::cmp::Reverse((next.y, next.x)),
                    ));
                }
            }
        }
        Ok(false)
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

/// Reserve an equal bounded share of remaining work for each remaining region.
/// Unused work carries forward; a failed region cannot consume later shares.
fn certify_regions(
    cache: &mut StartPassabilityCache<'_>,
    regions: Vec<Vec<TileCoord>>,
    complete: bool,
) -> Result<StartSearchResult, EnvironmentPageError> {
    let count = regions.len();
    let mut limited = false;
    for (index, candidates) in regions.into_iter().enumerate() {
        let remaining = count - index;
        cache.exit_visit_limit =
            cache.exit_visits + (START_EXIT_VISITS - cache.exit_visits) / remaining;
        cache.exit_chunk_limit =
            cache.exit_chunks.len() + (START_SEARCH_CHUNKS - cache.exit_chunks.len()) / remaining;
        cache.exit_budget_exhausted = false;
        for candidate in candidates {
            if (cache.cancelled)() {
                return Ok(StartSearchResult::Cancelled);
            }
            let valid = valid_start(cache, candidate)?;
            if (cache.cancelled)() {
                return Ok(StartSearchResult::Cancelled);
            }
            if valid {
                return Ok(StartSearchResult::Found(candidate));
            }
            if cache.exit_budget_exhausted {
                limited = true;
                break;
            }
        }
    }
    Ok(if (cache.cancelled)() {
        StartSearchResult::Cancelled
    } else if complete && !limited {
        StartSearchResult::Unavailable
    } else {
        StartSearchResult::LimitReached
    })
}

fn valid_start(
    cache: &mut StartPassabilityCache<'_>,
    candidate: TileCoord,
) -> Result<bool, EnvironmentPageError> {
    Ok(clear_starting_area(cache, candidate)?
        && cache.reaches_required_tiles(candidate)?
        && cache.reaches_exit(candidate)?)
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

#[path = "start_search/regions.rs"]
mod regions;

#[cfg(test)]
#[path = "start_search/tests/start_search.rs"]
mod tests;
