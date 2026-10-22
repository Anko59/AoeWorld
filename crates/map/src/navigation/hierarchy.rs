//! Hierarchical route search over 32 x 32 chunks (HPA*). Entrances are pairs
//! of open tiles across a chunk border (one portal per open run); intra-chunk
//! costs between a chunk's portals are computed lazily and cached with the
//! portal list. A route is planned over portals first; the caller refines each
//! hop locally with the exact tile planner. The graph is built from static
//! terrain and static resource nodes only (never the dynamic overlay), so a
//! result depends on the inputs alone, never on cache state. The search is
//! bounded by an abstract-node cap and is deterministic: ties break on tile
//! coordinates.
use super::{DIAGONAL_COST, ORTHOGONAL_COST, heuristic};
use crate::{CHUNK_TILES, EdgePassability, EnvironmentPageError, MapChunkGenerator};
use aoe_core::TileCoord;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap};
use std::sync::{Arc, OnceLock};

/// Upper bound for the retained portal lists and cost matrices of one
/// generator, the same budget as the walk cache.
pub(crate) const PORTAL_CACHE_MAX_BYTES: usize = 64 << 20;
/// Default abstract-node expansion bound of one search.
pub const DEFAULT_HIERARCHY_EXPANSIONS: u32 = 8_192;
/// Weighted A*: the portal lattice makes an exact search fan out over a wide
/// band of chunks, and touching a chunk is the expensive part. A route found
/// this way costs at most this many times the optimum, which is enough to
/// guide local refinement and to prove connectivity.
const HEURISTIC_WEIGHT: u64 = 2;
type Key = (i32, i32);

/// Chunk portal lists keyed by chunk, bounded by bytes with least-recently-used
/// eviction. A cost matrix is computed lazily after insertion, so an entry is
/// charged for its full `portals x portals` matrix up front.
#[derive(Debug)]
pub(crate) struct PortalCache {
    chunks: HashMap<Key, (Arc<ChunkPortals>, u64)>,
    bytes: usize,
    clock: u64,
    limit: usize,
}

impl Default for PortalCache {
    fn default() -> Self {
        Self::with_limit(PORTAL_CACHE_MAX_BYTES)
    }
}

impl PortalCache {
    pub(crate) fn with_limit(limit: usize) -> Self {
        Self {
            chunks: HashMap::new(),
            bytes: 0,
            clock: 0,
            limit,
        }
    }

    fn chunk(&mut self, key: Key) -> Option<Arc<ChunkPortals>> {
        self.clock += 1;
        let clock = self.clock;
        self.chunks.get_mut(&key).map(|(found, stamp)| {
            *stamp = clock;
            Arc::clone(found)
        })
    }

    pub(crate) fn insert(&mut self, key: Key, portals: Arc<ChunkPortals>) {
        self.clock += 1;
        self.bytes += portals.bytes();
        if let Some((old, _)) = self.chunks.insert(key, (portals, self.clock)) {
            self.bytes -= old.bytes();
        }
        if self.bytes > self.limit {
            let mut stamps: Vec<_> = self.chunks.iter().map(|(k, (_, s))| (*s, *k)).collect();
            stamps.sort_unstable();
            for (_, key) in stamps {
                if self.bytes <= self.limit / 10 * 9 {
                    break;
                }
                if let Some((old, _)) = self.chunks.remove(&key) {
                    self.bytes -= old.bytes();
                }
            }
        }
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.bytes
    }

    pub(crate) fn len(&self) -> usize {
        self.chunks.len()
    }
}

/// An entrance seen from one chunk: `tile` is inside it, `cross` is the open
/// tile on the other side of the border.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Portal {
    tile: TileCoord,
    cross: TileCoord,
}

#[derive(Debug)]
pub(crate) struct ChunkPortals {
    portals: Vec<Portal>,
    /// Row-major `portals x portals` costs, computed on first use.
    costs: OnceLock<Vec<Option<u32>>>,
}

impl ChunkPortals {
    fn bytes(&self) -> usize {
        let count = self.portals.capacity();
        std::mem::size_of::<Self>()
            + count * std::mem::size_of::<Portal>()
            + count * count * std::mem::size_of::<Option<u32>>()
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

struct Ground<'a>(&'a MapChunkGenerator);

impl Ground<'_> {
    /// Ground that can ever be walked. A resource node does not close its
    /// tile here: a depleted node opens it, the static graph cannot know which
    /// nodes are depleted, and `Unreachable` must stay a proof, so it is
    /// optimistic. The exact tile planner still honours the overlay.
    fn open(&self, tile: TileCoord) -> Result<Option<(bool, bool)>> {
        Ok(self
            .0
            .walk_at_with_cancel(tile, &|| false)?
            .map(|cell| (cell.passable, cell.mud)))
    }

    fn is_open(&self, tile: TileCoord) -> Result<bool> {
        Ok(self.open(tile)?.is_some_and(|(open, _)| open))
    }

    fn edge(&self, from: TileCoord, to: TileCoord) -> Result<bool> {
        Ok(matches!(
            self.0.edge_between_with_cancel(from, to, &|| false)?,
            EdgePassability::Passable
        ))
    }

    /// The exact tile planner's step rule, on static ground.
    fn step(&self, from: TileCoord, to: TileCoord) -> Result<Option<u32>> {
        let Some((open, mud)) = self.open(to)? else {
            return Ok(None);
        };
        if !open || !self.edge(from, to)? {
            return Ok(None);
        }
        if from.x != to.x && from.y != to.y {
            for corner in [TileCoord::new(to.x, from.y), TileCoord::new(from.x, to.y)] {
                if !self.is_open(corner)? || !self.edge(from, corner)? {
                    return Ok(None);
                }
            }
        }
        Ok(Some(step_cost(from, to, mud)))
    }

    /// Dijkstra confined to one chunk from `source`; cost to every reached tile.
    fn local_costs(&self, key: Key, source: TileCoord) -> Result<BTreeMap<TileCoord, u32>> {
        let mut best = BTreeMap::from([(source, 0_u32)]);
        let mut heap = BinaryHeap::from([Reverse((0_u32, source.y, source.x))]);
        while let Some(Reverse((cost, y, x))) = heap.pop() {
            let tile = TileCoord::new(x, y);
            if best.get(&tile) != Some(&cost) {
                continue;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let next = TileCoord::new(x + dx, y + dy);
                    if (dx == 0 && dy == 0) || !inside(key, next) {
                        continue;
                    }
                    let Some(step) = self.step(tile, next)? else {
                        continue;
                    };
                    let total = cost + step;
                    if best.get(&next).is_none_or(|known| *known > total) {
                        best.insert(next, total);
                        heap.push(Reverse((total, next.y, next.x)));
                    }
                }
            }
        }
        Ok(best)
    }

    fn build_portals(&self, key: Key) -> Result<ChunkPortals> {
        let origin = TileCoord::new(key.0 * CHUNK_TILES, key.1 * CHUNK_TILES);
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
                portals.push(Portal { tile, cross });
            }
        }
        Ok(ChunkPortals {
            portals,
            costs: OnceLock::new(),
        })
    }

    fn portals(&self, key: Key) -> Result<Arc<ChunkPortals>> {
        if let Ok(mut cache) = self.0.portal_cache().lock()
            && let Some(found) = cache.chunk(key)
        {
            return Ok(found);
        }
        let built = Arc::new(self.build_portals(key)?);
        if let Ok(mut cache) = self.0.portal_cache().lock() {
            cache.insert(key, Arc::clone(&built));
        }
        Ok(built)
    }

    fn intra<'a>(&self, key: Key, chunk: &'a ChunkPortals) -> Result<&'a [Option<u32>]> {
        if chunk.costs.get().is_none() {
            let count = chunk.portals.len();
            let mut matrix = vec![None; count * count];
            for (from, portal) in chunk.portals.iter().enumerate() {
                let reached = self.local_costs(key, portal.tile)?;
                for (to, other) in chunk.portals.iter().enumerate() {
                    matrix[from * count + to] = reached.get(&other.tile).copied();
                }
            }
            let _ = chunk.costs.set(matrix);
        }
        // SAFETY of unwrap: set above or by another thread.
        Ok(chunk.costs.get().map_or(&[][..], Vec::as_slice))
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
    let ground = Ground(generator);
    if !ground.is_open(origin)? || !ground.is_open(destination)? {
        return Ok(HierarchicalOutcome::Invalid);
    }
    let (start_key, goal_key) = (chunk_of(origin), chunk_of(destination));
    let start_costs = ground.local_costs(start_key, origin)?;
    if start_key == goal_key
        && let Some(cost) = start_costs.get(&destination)
    {
        return Ok(HierarchicalOutcome::Path {
            waypoints: Vec::new(),
            cost: u64::from(*cost),
        });
    }
    let goal_costs = ground.local_costs(goal_key, destination)?;
    let mut best: BTreeMap<Node, u64> = BTreeMap::from([(Node::Start, 0)]);
    let mut parent: BTreeMap<Node, Node> = BTreeMap::new();
    // Equal estimates expand the node with the greater actual cost first, as
    // the tile planner does, so open ground is crossed in a straight corridor.
    let mut heap = BinaryHeap::from([Reverse((
        HEURISTIC_WEIGHT * heuristic(origin, destination),
        u64::MAX,
        Node::Start,
    ))]);
    let mut expansions = 0_u32;
    let position = |node: Node| match node {
        Node::Start => origin,
        Node::Goal => destination,
        Node::Portal(tile) => tile,
    };
    while let Some(Reverse((_, inverse_cost, node))) = heap.pop() {
        let cost = u64::MAX - inverse_cost;
        if best.get(&node) != Some(&cost) {
            continue;
        }
        if node == Node::Goal {
            let mut waypoints = Vec::new();
            let mut at = node;
            while let Some(&before) = parent.get(&at) {
                if let Node::Portal(tile) = at {
                    waypoints.push(tile);
                }
                at = before;
            }
            waypoints.reverse();
            return Ok(HierarchicalOutcome::Path { waypoints, cost });
        }
        expansions += 1;
        if expansions > max_expansions {
            return Ok(HierarchicalOutcome::Bounded);
        }
        let mut edges: Vec<(Node, u64)> = Vec::new();
        match node {
            Node::Start => {
                for portal in &ground.portals(start_key)?.portals {
                    if let Some(cost) = start_costs.get(&portal.tile) {
                        edges.push((Node::Portal(portal.tile), u64::from(*cost)));
                    }
                }
            }
            Node::Goal => {}
            Node::Portal(tile) => {
                let key = chunk_of(tile);
                let chunk = ground.portals(key)?;
                let costs = ground.intra(key, &chunk)?;
                let count = chunk.portals.len();
                for (from, here) in chunk.portals.iter().enumerate() {
                    if here.tile != tile {
                        continue;
                    }
                    for (to, other) in chunk.portals.iter().enumerate() {
                        if to != from
                            && let Some(Some(cost)) = costs.get(from * count + to)
                        {
                            edges.push((Node::Portal(other.tile), u64::from(*cost)));
                        }
                    }
                    let mud = ground.open(here.cross)?.is_some_and(|(_, mud)| mud);
                    edges.push((
                        Node::Portal(here.cross),
                        u64::from(step_cost(here.tile, here.cross, mud)),
                    ));
                }
                if key == goal_key
                    && let Some(cost) = goal_costs.get(&tile)
                {
                    edges.push((Node::Goal, u64::from(*cost)));
                }
            }
        }
        for (next, step) in edges {
            let total = cost + step;
            if best.get(&next).is_none_or(|known| *known > total) {
                best.insert(next, total);
                parent.insert(next, node);
                heap.push(Reverse((
                    total + HEURISTIC_WEIGHT * heuristic(position(next), destination),
                    u64::MAX - total,
                    next,
                )));
            }
        }
    }
    Ok(HierarchicalOutcome::Unreachable)
}

#[cfg(test)]
mod tests;
