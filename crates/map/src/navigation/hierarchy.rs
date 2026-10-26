//! Hierarchical route search over 32 x 32 chunks (HPA*). Entrances are pairs
//! of open tiles across a chunk border (one portal per open run); intra-chunk
//! costs and exact local land components are cached per chunk state.
//! Native match preparation installs immutable terrain, components and costs.
//! Only overlay changes rebuild local graphs under the planner budgets. A route
//! is planned over portals first; the caller refines each
//! hop locally with the exact tile planner. Spatial overlay fingerprints
//! update affected chunks after building, wall and depletion changes. Results
//! depend on inputs alone, never on cache state. The search is
//! bounded by an abstract-node cap and is deterministic: ties break on tile
//! coordinates.
use super::{DIAGONAL_COST, ORTHOGONAL_COST, heuristic};
use crate::{CHUNK_TILES, EnvironmentPageError, MapChunkGenerator, ResourceOverlay, WalkCell};
use aoe_core::TileCoord;
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap};
use std::sync::{Arc, OnceLock};
mod grid;
mod static_data;
pub use static_data::NavigationSnapshot;
pub use static_data::{NAVIGATION_FORMAT_VERSION, NavigationChunk, navigation_checksum};
mod prepared;
pub(crate) use prepared::{Resolution, ResolutionOutcome};

/// Upper bound for the retained portal lists and cost matrices of one
/// generator, the same budget as the walk cache.
pub(crate) const PORTAL_CACHE_MAX_BYTES: usize = 64 << 20;
/// Bounded nearest-destination memo entries per collision snapshot.
const MAX_CACHED_CHUNKS: usize = 262_144;
/// Default abstract-node expansion bound of one search.
pub const DEFAULT_HIERARCHY_EXPANSIONS: u32 = 8_192;
/// Weighted A*: the portal lattice makes an exact search fan out over a wide
/// band of chunks, and touching a chunk is the expensive part. A route found
/// this way costs at most this many times the optimum, which is enough to
/// guide local refinement and to prove connectivity.
const HEURISTIC_WEIGHT: u64 = 2;
type Key = (i32, i32);

mod cache;
pub(crate) use cache::PortalCache;

/// An entrance seen from one chunk: `tile` is inside it, `cross` is the open
/// tile on the other side of the border.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct Portal {
    tile: TileCoord,
    cross: TileCoord,
    crossing_cost: u32,
    bidirectional: bool,
}

#[derive(Debug)]
pub(crate) struct ChunkPortals {
    portals: Vec<Portal>,
    /// Filled on the first route through this chunk, after connectivity queries.
    costs: OnceLock<Vec<Vec<Option<u32>>>>,
    labels: grid::Labels,
    components: u16,
    links: Vec<(u16, u16)>,
}

impl ChunkPortals {
    fn bytes(&self) -> usize {
        let count = self.portals.capacity();
        std::mem::size_of::<Self>()
            + count * std::mem::size_of::<Portal>()
            + count * count * std::mem::size_of::<Option<u32>>()
            + self.labels.bytes()
            + self.links.capacity() * std::mem::size_of::<(u16, u16)>()
            + count * std::mem::size_of::<Vec<Option<u32>>>()
            + 64
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HierarchicalOutcome {
    /// Portal tiles to walk through, in order (each pair is an entrance:
    /// the tile before the border, then the first tile after it), and the
    /// abstract cost of the whole route.
    Path {
        waypoints: Vec<TileCoord>,
        cost: u64,
    },
    /// Every reachable portal was exhausted: origin and goal are disconnected
    /// in the static terrain.
    Unreachable,
    /// The expansion bound ended the search first.
    Bounded,
    /// Origin or goal is not open ground.
    Invalid,
}

fn chunk_of(tile: TileCoord) -> Key {
    (
        tile.x.div_euclid(CHUNK_TILES),
        tile.y.div_euclid(CHUNK_TILES),
    )
}

fn inside(key: Key, tile: TileCoord) -> bool {
    chunk_of(tile) == key
}

/// (inside tile, outside tile) at offset `i` along border `side` (east, south,
/// west, north) of the chunk at `origin`.
fn border_pair(side: u8, origin: TileCoord, i: i32) -> (TileCoord, TileCoord) {
    let last = CHUNK_TILES - 1;
    match side {
        0 => (
            TileCoord::new(origin.x + last, origin.y + i),
            TileCoord::new(origin.x + last + 1, origin.y + i),
        ),
        1 => (
            TileCoord::new(origin.x + i, origin.y + last),
            TileCoord::new(origin.x + i, origin.y + last + 1),
        ),
        2 => (
            TileCoord::new(origin.x, origin.y + i),
            TileCoord::new(origin.x - 1, origin.y + i),
        ),
        _ => (
            TileCoord::new(origin.x + i, origin.y),
            TileCoord::new(origin.x + i, origin.y - 1),
        ),
    }
}

fn step_cost(from: TileCoord, to: TileCoord, mud: bool) -> u32 {
    let base = if from.x == to.x || from.y == to.y {
        ORTHOGONAL_COST
    } else {
        DIAGONAL_COST
    };
    base * if mud { 3 } else { 2 } / 2
}

type Result<T> = std::result::Result<T, EnvironmentPageError>;

/// `(start, end)` index ranges (end exclusive) of one border's entrances: runs
/// of open pairs in which every neighbour is walkable to the next on both
/// sides (`linked[i]` joins pair `i - 1` to pair `i`). A height step or wall
/// inside a run of open pairs splits it, so each part keeps its own portal.
pub(super) fn entrance_runs(open: &[bool], linked: &[bool]) -> Vec<(i32, i32)> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for i in 0..=open.len() {
        let continues = i < open.len() && open[i] && (start.is_none() || linked[i]);
        if !continues && let Some(from) = start.take() {
            runs.push((from as i32, i as i32));
        }
        if i < open.len() && open[i] && start.is_none() {
            start = Some(i);
        }
    }
    runs
}

struct Ground<'a> {
    generator: &'a MapChunkGenerator,
    cancelled: &'a dyn Fn() -> bool,
    overlay: &'a ResourceOverlay,
    walk: RefCell<HashMap<TileCoord, Option<WalkCell>>>,
    atlas: Option<Arc<prepared::Atlas>>,
}

impl<'a> Ground<'a> {
    fn new(
        generator: &'a MapChunkGenerator,
        overlay: &'a ResourceOverlay,
        cancelled: &'a dyn Fn() -> bool,
    ) -> Self {
        Self {
            generator,
            cancelled,
            overlay,
            walk: RefCell::default(),
            atlas: None,
        }
    }
}

impl Ground<'_> {
    fn open(&self, tile: TileCoord) -> Result<Option<WalkCell>> {
        if let Some(value) = self.walk.borrow().get(&tile) {
            return Ok(*value);
        }
        let value = self.generator.walk_at_with_cancel(tile, self.cancelled)?;
        self.walk.borrow_mut().insert(tile, value);
        Ok(value)
    }

    fn is_open(&self, tile: TileCoord) -> Result<bool> {
        Ok(self.open(tile)?.is_some_and(|cell| {
            (cell.passable || self.overlay.bridge_axis(tile).is_some())
                && !self.overlay.blocks_tile(tile)
                && cell.node.is_none_or(|node| !self.overlay.blocks_node(node))
        }))
    }

    fn edge(&self, from: TileCoord, to: TileCoord) -> Result<bool> {
        if let Some(clear) = self.overlay.bridge_edge(from, to) {
            return Ok(clear);
        }
        let (Some(a), Some(b)) = (self.open(from)?, self.open(to)?) else {
            return Ok(false);
        };
        Ok(a.passable
            && b.passable
            && a.walkable_surface
            && b.walkable_surface
            && (i32::from(a.game_height_level) - i32::from(b.game_height_level)).abs() <= 1)
    }

    fn diagonal_transition(&self, from: TileCoord, to: TileCoord) -> Result<Option<u32>> {
        if !self.is_open(from)? || !self.is_open(to)? || !self.edge(from, to)? {
            return Ok(None);
        }
        let corners = [TileCoord::new(to.x, from.y), TileCoord::new(from.x, to.y)];
        for corner in corners {
            if !self.is_open(corner)? || !self.edge(from, corner)? {
                return Ok(None);
            }
        }
        for corner in corners {
            if self.edge(corner, to)? {
                return Ok(None);
            }
        }
        Ok(Some(step_cost(
            from,
            to,
            self.open(to)?.is_some_and(|cell| cell.mud),
        )))
    }

    /// Dijkstra confined to one chunk from `source`; cost to every reached tile.
    fn local_costs(&self, key: Key, source: TileCoord) -> Result<BTreeMap<TileCoord, u32>> {
        let costs = grid::Grid::new(self, key)?.costs(source, self.cancelled)?;
        Ok(costs
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, cost)| *cost != u32::MAX)
            .map(|(at, cost)| (grid::tile(key, at), cost))
            .collect())
    }

    fn goal_costs(&self, key: Key, source: TileCoord) -> Result<BTreeMap<TileCoord, u32>> {
        let costs = grid::Grid::new(self, key)?.costs_to(source, self.cancelled)?;
        Ok(costs
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, cost)| *cost != u32::MAX)
            .map(|(at, cost)| (grid::tile(key, at), cost))
            .collect())
    }

    fn build_portals(&self, key: Key) -> Result<ChunkPortals> {
        let _span = self
            .generator
            .measure_navigation(crate::NavigationPhase::Portals);
        let origin = TileCoord::new(key.0 * CHUNK_TILES, key.1 * CHUNK_TILES);
        // Loaded navigation already contains all water/passability facts.
        // Dynamic rebuilds must never fall back to cold source residency.
        if !self.generator.has_static_navigation()
            && !self.overlay.bridge_in_chunk(key)
            && self.generator.open_water_certain_near(
                TileCoord::new(origin.x + CHUNK_TILES / 2, origin.y + CHUNK_TILES / 2),
                CHUNK_TILES / 2,
                self.cancelled,
            )?
        {
            return Ok(ChunkPortals {
                portals: Vec::new(),
                costs: OnceLock::new(),
                labels: grid::Labels::Empty,
                components: 0,
                links: Vec::new(),
            });
        }
        let mut portals = Vec::new();
        for side in 0..4 {
            let border = |i| border_pair(side, origin, i);
            let mut open = [false; CHUNK_TILES as usize];
            let mut linked = [false; CHUNK_TILES as usize];
            for i in 0..CHUNK_TILES {
                let (a, b) = border(i);
                open[i as usize] = self.is_open(a)? && self.is_open(b)? && self.edge(a, b)?;
                if i > 0 && open[i as usize] && open[i as usize - 1] {
                    let (before_a, before_b) = border(i - 1);
                    linked[i as usize] = self.edge(before_a, a)? && self.edge(before_b, b)?;
                }
            }
            for (start, end) in entrance_runs(&open, &linked) {
                let (tile, cross) = border((start + end - 1) / 2);
                portals.push(Portal {
                    tile,
                    cross,
                    crossing_cost: step_cost(
                        tile,
                        cross,
                        self.open(cross)?.is_some_and(|cell| cell.mud),
                    ),
                    bidirectional: true,
                });
            }
        }
        let grid = grid::Grid::new(self, key)?;
        // Only diagonal border transitions that have no two-cardinal-step
        // equivalent need extra portals. Most chunks have none.
        for at in 0..grid::CELLS {
            let from = grid::tile(key, at);
            if from.x != origin.x
                && from.x != origin.x + CHUNK_TILES - 1
                && from.y != origin.y
                && from.y != origin.y + CHUNK_TILES - 1
            {
                continue;
            }
            if !self.is_open(from)? {
                continue;
            }
            for (dx, dy) in [(-1, -1), (-1, 1), (1, -1), (1, 1)] {
                let to = TileCoord::new(from.x + dx, from.y + dy);
                if inside(key, to) {
                    continue;
                }
                // On a uniform-height chunk a side crossing always has a
                // cardinal equivalent through its local corner. Corner-to-
                // corner chunk crossings still need their exact check.
                if grid.uniform_height && (chunk_of(to).0 == key.0 || chunk_of(to).1 == key.1) {
                    continue;
                }
                if let Some(cost) = self.diagonal_transition(from, to)? {
                    portals.push(Portal {
                        tile: from,
                        cross: to,
                        crossing_cost: cost,
                        bidirectional: false,
                    });
                }
                if self.diagonal_transition(to, from)?.is_some() {
                    portals.push(Portal {
                        tile: from,
                        cross: to,
                        crossing_cost: 0,
                        bidirectional: false,
                    });
                }
            }
        }
        let (labels, components, links) = grid.components();
        Ok(ChunkPortals {
            costs: OnceLock::new(),
            portals,
            labels,
            components,
            links,
        })
    }

    fn portals(&self, key: Key) -> Result<Arc<ChunkPortals>> {
        if let Some(atlas) = &self.atlas {
            return atlas.chunk(self.generator, key, self.cancelled);
        }
        let stamp = self.overlay.navigation_key(key);
        if stamp == ResourceOverlay::default().navigation_key(key)
            && let Some(chunk) = self.generator.static_navigation_chunk(key)
        {
            return Ok(Arc::clone(&chunk.graph));
        }
        if let Ok(mut cache) = self.generator.portal_cache().lock()
            && let Some(found) = cache.chunk(key, stamp)
        {
            return Ok(found);
        }
        let built = Arc::new(self.build_portals(key)?);
        if let Ok(mut cache) = self.generator.portal_cache().lock() {
            cache.insert(key, stamp, Arc::clone(&built));
        }
        Ok(built)
    }

    fn row<'a>(
        &self,
        key: Key,
        chunk: &'a ChunkPortals,
        source: TileCoord,
    ) -> Result<&'a [Option<u32>]> {
        let Some(from) = chunk
            .portals
            .iter()
            .position(|portal| portal.tile == source)
        else {
            return Ok(&[]);
        };
        if chunk.costs.get().is_none() {
            let _span = self
                .generator
                .measure_navigation(crate::NavigationPhase::PortalCosts);
            let overlay = self
                .atlas
                .as_ref()
                .map_or(self.overlay, |atlas| atlas.overlay());
            let ground = Ground::new(self.generator, overlay, self.cancelled);
            let grid = grid::Grid::new(&ground, key)?;
            let mut costs = Vec::with_capacity(chunk.portals.len());
            for source in &chunk.portals {
                let reached = grid.costs(source.tile, self.cancelled)?;
                costs.push(
                    chunk
                        .portals
                        .iter()
                        .map(|portal| {
                            let cost = reached[grid::index(portal.tile)];
                            (cost != u32::MAX).then_some(cost)
                        })
                        .collect(),
                );
            }
            let _ = chunk.costs.set(costs);
        }
        chunk
            .costs
            .get()
            .and_then(|costs| costs.get(from))
            .map(Vec::as_slice)
            .ok_or(EnvironmentPageError::Unavailable)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Node {
    Start,
    Portal(TileCoord),
    Goal,
}

/// Plans `origin` to `destination` over chunk portals.
pub fn hierarchical_route(
    generator: &MapChunkGenerator,
    origin: TileCoord,
    destination: TileCoord,
    max_expansions: u32,
) -> Result<HierarchicalOutcome> {
    hierarchical_route_with_overlay(
        generator,
        &ResourceOverlay::default(),
        origin,
        destination,
        max_expansions,
    )
}

/// Portal guidance over the current collision and bridge snapshot.
pub fn hierarchical_route_with_overlay(
    generator: &MapChunkGenerator,
    overlay: &ResourceOverlay,
    origin: TileCoord,
    destination: TileCoord,
    max_expansions: u32,
) -> Result<HierarchicalOutcome> {
    let mut search = incremental::Search::new(origin, destination, max_expansions);
    loop {
        if let Some(outcome) = search.poll_overlay(generator, overlay, &mut 65_536, &|| false)? {
            return Ok(outcome);
        }
    }
}

mod incremental;
pub(crate) use incremental::Search;

#[cfg(test)]
mod tests;
