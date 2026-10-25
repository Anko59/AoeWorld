//! Chunk-level walk data: passability, walkable surface, height level, mud and
//! resource-node bitmaps of a whole 32 x 32 chunk, prepared explicitly by
//! navigation or background workers and shared by movement, route planning,
//! gathering and placement. Cold point reads stay local. The cache evicts
//! the least recently used chunks. Every value is a pure function of the
//! immutable generator, so hits and misses can never change a result.
use super::{EdgePassability, GroundMaterial, MapChunkGenerator, ResourceNode};
use crate::CHUNK_TILES;
use crate::EnvironmentPageError;
use aoe_core::TileCoord;
use std::collections::HashMap;
use std::sync::Arc;

mod probe;
pub use probe::{NavigationPhase, NavigationProbe, NavigationSpan};

const SIDE: usize = CHUNK_TILES as usize;
const CELLS: usize = SIDE * SIDE;
const WORDS: usize = CELLS / 64;
/// Upper bound for the retained chunk bitmaps of one generator.
pub const WALK_CACHE_MAX_BYTES: usize = 64 << 20;

/// What movement and route planning need to know about one tile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WalkCell {
    /// Composed ground passability, before resource and building blockers.
    pub passable: bool,
    pub walkable_surface: bool,
    pub game_height_level: i16,
    pub mud: bool,
    pub node: Option<ResourceNode>,
}

#[derive(Debug)]
pub(super) struct WalkChunk {
    present: [u64; WORDS],
    passable: [u64; WORDS],
    walkable: [u64; WORDS],
    mud: [u64; WORDS],
    levels: Box<[i16; CELLS]>,
    tiles: Box<[Option<super::Tile>; CELLS]>,
    /// `(local index, node)` ordered by local index.
    nodes: Vec<(u16, ResourceNode)>,
}

fn get(bits: &[u64; WORDS], index: usize) -> bool {
    bits[index / 64] >> (index % 64) & 1 == 1
}

fn set(bits: &mut [u64; WORDS], index: usize) {
    bits[index / 64] |= 1 << (index % 64);
}

fn local(tile: TileCoord) -> usize {
    tile.y.rem_euclid(CHUNK_TILES) as usize * SIDE + tile.x.rem_euclid(CHUNK_TILES) as usize
}

impl WalkChunk {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + CELLS * 2
            + CELLS * std::mem::size_of::<Option<super::Tile>>()
            + self.nodes.capacity() * std::mem::size_of::<(u16, ResourceNode)>()
            + 64
    }

    fn cell(&self, tile: TileCoord) -> Option<WalkCell> {
        let index = local(tile);
        get(&self.present, index).then(|| WalkCell {
            passable: get(&self.passable, index),
            walkable_surface: get(&self.walkable, index),
            game_height_level: self.levels[index],
            mud: get(&self.mud, index),
            node: self
                .nodes
                .binary_search_by_key(&(index as u16), |(at, _)| *at)
                .ok()
                .map(|found| self.nodes[found].1),
        })
    }
}

/// Per-generator derived caches, shared by clones.
#[derive(Debug)]
pub(super) struct DerivedCaches {
    /// Process-unique, so a thread's recent chunks can never serve another
    /// generator, even one allocated at the same address.
    id: u64,
    pub(crate) static_navigation: std::sync::OnceLock<Arc<crate::navigation::NavigationSnapshot>>,

    pub(super) probe: std::sync::RwLock<Option<Arc<dyn NavigationProbe>>>,
    pub(super) point: std::sync::Mutex<super::point_memo::PointMemo>,
    pub(super) walk: std::sync::Mutex<WalkCache>,
    pub(super) portals: std::sync::Mutex<crate::navigation::PortalCache>,
}

impl Default for DerivedCaches {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            static_navigation: Default::default(),
            probe: Default::default(),
            point: Default::default(),
            walk: Default::default(),
            portals: Default::default(),
        }
    }
}

type Recent = [Option<(u64, (i32, i32), Arc<WalkChunk>)>; RECENT_CHUNKS];
/// Chunks each thread keeps beside the shared cache: parallel readers of
/// neighbouring tiles then skip the lock and the shared reference count.
const RECENT_CHUNKS: usize = 4;

thread_local! {
    static RECENT: std::cell::RefCell<(Recent, usize)> =
        const { std::cell::RefCell::new(([const { None }; RECENT_CHUNKS], 0)) };
}

#[derive(Debug, Default)]
pub(super) struct WalkCache {
    chunks: HashMap<(i32, i32), (Arc<WalkChunk>, u64)>,
    bytes: usize,
    clock: u64,
}

impl WalkCache {
    fn get(&mut self, key: (i32, i32)) -> Option<Arc<WalkChunk>> {
        self.clock += 1;
        let clock = self.clock;
        self.chunks.get_mut(&key).map(|(chunk, stamp)| {
            *stamp = clock;
            Arc::clone(chunk)
        })
    }

    fn insert(&mut self, key: (i32, i32), chunk: Arc<WalkChunk>) {
        self.clock += 1;
        self.bytes += chunk.bytes();
        if let Some((old, _)) = self.chunks.insert(key, (chunk, self.clock)) {
            self.bytes -= old.bytes();
        }
        if self.bytes > WALK_CACHE_MAX_BYTES {
            let mut stamps: Vec<_> = self.chunks.iter().map(|(k, (_, s))| (*s, *k)).collect();
            stamps.sort_unstable();
            for (_, key) in stamps {
                if self.bytes <= WALK_CACHE_MAX_BYTES / 10 * 9 {
                    break;
                }
                if let Some((old, _)) = self.chunks.remove(&key) {
                    self.bytes -= old.bytes();
                }
            }
        }
    }

    pub(super) fn retained_bytes(&self) -> usize {
        self.bytes
    }

    pub(super) fn len(&self) -> usize {
        self.chunks.len()
    }
}

/// Result of a cache lookup that never hides a provider failure: `Slow` tells
/// the caller to use the exact per-tile path, which reports the error for the
/// one tile that has it.
pub(super) enum Lookup {
    OutOfMap,
    Cell(WalkCell),
    Slow,
}

impl MapChunkGenerator {
    pub(crate) fn static_navigation(
        &self,
    ) -> &std::sync::OnceLock<Arc<crate::navigation::NavigationSnapshot>> {
        &self.derived.static_navigation
    }

    /// Isolated scratch caches for an adapter's parallel preparation worker.
    pub fn navigation_preparation_worker(&self) -> Self {
        let mut worker = self.clone();
        worker.derived = Default::default();
        worker.river_queries = Default::default();
        worker
    }

    pub fn width_tiles(&self) -> i32 {
        self.width_tiles
    }

    /// Compose an immutable walk chunk on an adapter-owned preparation worker.
    pub fn precompose_walk_chunk(
        &self,
        x: i32,
        y: i32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), EnvironmentPageError> {
        self.walk_chunk_observed((x, y), cancelled, false)
            .map(|_| ())
    }

    pub fn walk_chunk_resident(&self, x: i32, y: i32) -> bool {
        self.derived
            .walk
            .lock()
            .is_ok_and(|cache| cache.chunks.contains_key(&(x, y)))
    }

    fn walk_chunk(
        &self,
        chunk: (i32, i32),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Arc<WalkChunk>>, EnvironmentPageError> {
        self.walk_chunk_observed(chunk, cancelled, true)
    }

    fn walk_chunk_observed(
        &self,
        chunk: (i32, i32),
        cancelled: &dyn Fn() -> bool,
        observed: bool,
    ) -> Result<Option<Arc<WalkChunk>>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if let Ok(mut cache) = self.derived.walk.lock()
            && let Some(found) = cache.get(chunk)
        {
            return Ok(Some(found));
        }
        let measurement = observed
            .then(|| self.measure_navigation(NavigationPhase::Terrain))
            .flatten();
        let built = match self.build_walk_chunk(chunk, cancelled) {
            Ok(built) => Arc::new(built),
            Err(EnvironmentPageError::Cancelled) => return Err(EnvironmentPageError::Cancelled),
            Err(_) => return Ok(None),
        };
        drop(measurement);
        if let Ok(mut cache) = self.derived.walk.lock() {
            let _span = observed
                .then(|| self.measure_navigation(NavigationPhase::WalkCache))
                .flatten();
            cache.insert(chunk, Arc::clone(&built));
        }
        Ok(Some(built))
    }

    fn build_walk_chunk(
        &self,
        (x, y): (i32, i32),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<WalkChunk, EnvironmentPageError> {
        let scene = self.landscape_chunk_with_cancel(x, y, cancelled)?;
        let mut chunk = WalkChunk {
            present: [0; WORDS],
            passable: [0; WORDS],
            walkable: [0; WORDS],
            mud: [0; WORDS],
            levels: Box::new([0; CELLS]),
            tiles: Box::new([None; CELLS]),
            nodes: Vec::with_capacity(scene.resources.len()),
        };
        for tile in &scene.tiles {
            let index = local(tile.tile);
            let terrain = &tile.terrain;
            chunk.tiles[index] = Some(*terrain);
            set(&mut chunk.present, index);
            if terrain.passable {
                set(&mut chunk.passable, index);
            }
            if terrain.surface.walkable() {
                set(&mut chunk.walkable, index);
            }
            if terrain.material == GroundMaterial::Mud {
                set(&mut chunk.mud, index);
            }
            chunk.levels[index] = terrain.game_height_level;
        }
        for resource in &scene.resources {
            chunk
                .nodes
                .push((local(resource.node.tile) as u16, resource.node));
        }
        chunk.nodes.sort_unstable_by_key(|(at, _)| *at);
        Ok(chunk)
    }

    pub(super) fn walk_lookup(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Lookup, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if tile.x < 0 || tile.y < 0 || tile.x >= self.width_tiles || tile.y >= self.width_tiles {
            return Ok(Lookup::OutOfMap);
        }
        if let Some(cell) = self.static_navigation_cell(tile) {
            return Ok(cell.map_or(Lookup::OutOfMap, Lookup::Cell));
        }
        let key = (
            tile.x.div_euclid(CHUNK_TILES),
            tile.y.div_euclid(CHUNK_TILES),
        );
        let id = self.derived.id;
        let recent = RECENT.with(|recent| {
            recent
                .borrow()
                .0
                .iter()
                .flatten()
                .find_map(|(owner, at, chunk)| {
                    (*owner == id && *at == key)
                        .then(|| chunk.cell(tile).map_or(Lookup::OutOfMap, Lookup::Cell))
                })
        });
        if let Some(found) = recent {
            return Ok(found);
        }
        // Point reads only reuse explicitly prepared chunks; a miss stays local.
        let chunk = self
            .derived
            .walk
            .lock()
            .ok()
            .and_then(|mut cache| cache.get(key));
        Ok(match chunk {
            Some(chunk) => {
                let found = chunk.cell(tile).map_or(Lookup::OutOfMap, Lookup::Cell);
                RECENT.with(|recent| {
                    let (slots, next) = &mut *recent.borrow_mut();
                    slots[*next] = Some((id, key, chunk));
                    *next = (*next + 1) % RECENT_CHUNKS;
                });
                found
            }
            None => Lookup::Slow,
        })
    }

    /// The full point projection is already present in a prepared chunk.
    /// Reuse it even after the small direct-mapped point memo has collided.
    pub(super) fn resident_point(&self, tile: TileCoord) -> Option<super::point_memo::Point> {
        let key = (
            tile.x.div_euclid(CHUNK_TILES),
            tile.y.div_euclid(CHUNK_TILES),
        );
        let chunk = self.derived.walk.lock().ok()?.get(key)?;
        Some(chunk.tiles[local(tile)].map(|terrain| {
            let node = chunk.cell(tile).and_then(|cell| cell.node);
            (terrain, node)
        }))
    }

    /// Cached composed walk data of one tile; `None` outside the map.
    pub fn walk_at_with_cancel(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<WalkCell>, EnvironmentPageError> {
        match self.walk_lookup(tile, cancelled)? {
            Lookup::OutOfMap => Ok(None),
            Lookup::Cell(cell) => Ok(Some(cell)),
            Lookup::Slow => Ok(self
                .memoized_point(tile, cancelled)?
                .map(|(terrain, node)| WalkCell {
                    passable: terrain.passable,
                    walkable_surface: terrain.surface.walkable(),
                    game_height_level: terrain.game_height_level,
                    mud: terrain.material == GroundMaterial::Mud,
                    node,
                })),
        }
    }

    /// Edge rule between two adjacent tiles from cached chunk data.
    pub(super) fn cached_edge(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<EdgePassability>, EnvironmentPageError> {
        let (Lookup::Cell(a), Lookup::Cell(b)) = (
            self.walk_lookup(from, cancelled)?,
            self.walk_lookup(to, cancelled)?,
        ) else {
            return Ok(None);
        };
        Ok(Some(
            if !a.passable
                || !b.passable
                || !a.walkable_surface
                || !b.walkable_surface
                || (i32::from(a.game_height_level) - i32::from(b.game_height_level)).abs() > 1
            {
                EdgePassability::Blocked
            } else {
                EdgePassability::Passable
            },
        ))
    }

    pub(crate) fn navigation_cells(
        &self,
        key: (i32, i32),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<Option<WalkCell>>, EnvironmentPageError> {
        if self.has_static_navigation() {
            return (0..CELLS)
                .map(|index| {
                    self.walk_at_with_cancel(
                        TileCoord::new(
                            key.0 * CHUNK_TILES + (index % SIDE) as i32,
                            key.1 * CHUNK_TILES + (index / SIDE) as i32,
                        ),
                        cancelled,
                    )
                })
                .collect();
        }
        let chunk = self.walk_chunk(key, cancelled)?;
        (0..CELLS)
            .map(|index| {
                let tile = TileCoord::new(
                    key.0 * CHUNK_TILES + (index % SIDE) as i32,
                    key.1 * CHUNK_TILES + (index / SIDE) as i32,
                );
                match &chunk {
                    Some(chunk) => Ok(chunk.cell(tile)),
                    None => self.walk_at_with_cancel(tile, cancelled),
                }
            })
            .collect()
    }

    pub(crate) fn navigation_chunk_side(&self) -> i32 {
        self.width_tiles.div_euclid(CHUNK_TILES)
            + i32::from(self.width_tiles.rem_euclid(CHUNK_TILES) != 0)
    }

    pub(crate) fn portal_cache(&self) -> &std::sync::Mutex<crate::navigation::PortalCache> {
        &self.derived.portals
    }

    /// Bytes and chunks currently retained by the portal cache.
    pub fn portal_cache_usage(&self) -> (usize, usize) {
        self.derived
            .portals
            .lock()
            .map_or((0, 0), |cache| (cache.retained_bytes(), cache.len()))
    }

    /// Bytes and chunks currently retained by the walk cache.
    pub fn walk_cache_usage(&self) -> (usize, usize) {
        self.derived
            .walk
            .lock()
            .map_or((0, 0), |cache| (cache.retained_bytes(), cache.len()))
    }
}

#[cfg(test)]
mod tests;
