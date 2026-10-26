//! Immutable package navigation. Native adapters prepare chunks in parallel and
//! own persistence; this module has no filesystem, clocks or worker scheduling.
use super::*;
use serde::{Deserialize, Serialize};

pub const NAVIGATION_FORMAT_VERSION: u32 = 1;
const MAX_CHUNK_BYTES: usize = 4 << 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct WalkRun {
    end: u16,
    flags: u8,
    height: i16,
}

/// Global tile coordinates are implied by the chunk and local index. Keep
/// the full identity (including its slot) and every other resource attribute.
#[derive(Debug)]
struct PackedNode {
    id: u64,
    at: u16,
    amount: u16,
    kind: crate::ResourceKind,
    object: crate::ObjectKind,
    variant: u8,
}

impl PackedNode {
    fn new(at: u16, node: crate::ResourceNode) -> Self {
        Self {
            id: node.id,
            at,
            amount: node.initial_amount,
            kind: node.kind,
            object: node.object,
            variant: node.visual_variant,
        }
    }

    fn node(&self, key: Key) -> crate::ResourceNode {
        crate::ResourceNode {
            id: self.id,
            tile: grid::tile(key, usize::from(self.at)),
            kind: self.kind,
            object: self.object,
            initial_amount: self.amount,
            visual_variant: self.variant,
        }
    }
}

#[derive(Debug)]
pub struct NavigationChunk {
    key: Key,
    runs: Box<[WalkRun]>,
    nodes: Box<[PackedNode]>,
    pub(super) graph: Arc<ChunkPortals>,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    key: Key,
    runs: Vec<WalkRun>,
    nodes: Vec<(u16, crate::ResourceNode)>,
    portals: Vec<Portal>,
    costs: Vec<Vec<Option<u32>>>,
    labels: Vec<(u16, u16)>,
    components: u16,
    links: Vec<(u16, u16)>,
}

#[derive(Debug)]
pub struct NavigationSnapshot {
    side: i32,
    chunks: Vec<NavigationChunk>,
    bytes: usize,
}

impl NavigationSnapshot {
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }
}

impl NavigationChunk {
    /// Retained allocation capacities, excluding allocator bookkeeping.
    pub fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + self.runs.len() * size_of::<WalkRun>()
            + self.nodes.len() * size_of::<PackedNode>()
            + self.graph.bytes()
    }

    pub fn coordinates(&self) -> (i32, i32) {
        self.key
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut labels: Vec<(u16, u16)> = Vec::new();
        for (at, &label) in self.graph.labels.iter().enumerate() {
            if let Some(last) = labels.last_mut().filter(|last| last.1 == label) {
                last.0 = at as u16 + 1;
            } else {
                labels.push((at as u16 + 1, label));
            }
        }
        let stored = Stored {
            key: self.key,
            runs: self.runs.to_vec(),
            nodes: self
                .nodes
                .iter()
                .map(|node| (node.at, node.node(self.key)))
                .collect(),
            portals: self.graph.portals.clone(),
            costs: self.graph.costs.get().cloned().unwrap_or_default(),
            labels,
            components: self.graph.components,
            links: self.graph.links.clone(),
        };
        postcard::to_allocvec(&stored).map_err(|_| EnvironmentPageError::Invalid)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_CHUNK_BYTES {
            return Err(EnvironmentPageError::Invalid);
        }
        let (stored, rest): (Stored, _) =
            postcard::take_from_bytes(bytes).map_err(|_| EnvironmentPageError::Invalid)?;
        let maximum_side = crate::request::MAX_TILES_PER_SIDE.div_ceil(CHUNK_TILES as u64) as i32;
        if stored.key.0 < 0
            || stored.key.1 < 0
            || stored.key.0 >= maximum_side
            || stored.key.1 >= maximum_side
        {
            return Err(EnvironmentPageError::Invalid);
        }
        let n = stored.portals.len();
        let valid_runs = |ends: Vec<u16>| {
            ends.last() == Some(&(grid::CELLS as u16))
                && ends
                    .iter()
                    .copied()
                    .try_fold(0, |last, end| {
                        (end > last && end <= grid::CELLS as u16).then_some(end)
                    })
                    .is_some()
        };
        if !rest.is_empty()
            || !valid_runs(stored.runs.iter().map(|r| r.end).collect())
            || !valid_runs(stored.labels.iter().map(|r| r.0).collect())
            || stored.runs.iter().any(|r| r.flags > 15)
            || stored.components > grid::CELLS as u16
            || stored.labels.iter().any(|r| r.1 > stored.components)
            || stored.nodes.len() > grid::CELLS
            || stored.nodes.windows(2).any(|pair| pair[0].0 >= pair[1].0)
            || stored.nodes.iter().any(|(at, node)| {
                usize::from(*at) >= grid::CELLS
                    || node.tile != grid::tile(stored.key, usize::from(*at))
            })
            || n > grid::CELLS * 8
            || stored.costs.len() != n
            || stored.costs.iter().any(|row| row.len() != n)
            || stored.portals.iter().any(|p| {
                !inside(stored.key, p.tile)
                    || (i64::from(p.tile.x) - i64::from(p.cross.x)).abs() > 1
                    || (i64::from(p.tile.y) - i64::from(p.cross.y)).abs() > 1
                    || inside(stored.key, p.cross)
            })
            || stored.links.iter().any(|(a, b)| {
                *a == 0 || *b == 0 || *a > stored.components || *b > stored.components
            })
        {
            return Err(EnvironmentPageError::Invalid);
        }
        let labels = grid::Labels::from_runs(stored.labels);
        let costs = OnceLock::new();
        let _ = costs.set(stored.costs);
        Ok(Self {
            key: stored.key,
            runs: stored.runs.into_boxed_slice(),
            nodes: stored
                .nodes
                .into_iter()
                .map(|(at, node)| PackedNode::new(at, node))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            graph: Arc::new(ChunkPortals {
                portals: stored.portals,
                costs,
                labels,
                components: stored.components,
                links: stored.links,
            }),
        })
    }

    fn cell(&self, at: usize) -> Option<WalkCell> {
        let run = &self.runs[self.runs.partition_point(|run| usize::from(run.end) <= at)];
        (run.flags & 1 != 0).then(|| WalkCell {
            passable: run.flags & 2 != 0,
            walkable_surface: run.flags & 4 != 0,
            mud: run.flags & 8 != 0,
            game_height_level: run.height,
            node: self
                .nodes
                .binary_search_by_key(&(at as u16), |node| node.at)
                .ok()
                .map(|index| self.nodes[index].node(self.key)),
        })
    }
}

impl MapChunkGenerator {
    pub fn navigation_snapshot(&self) -> Option<Arc<NavigationSnapshot>> {
        self.static_navigation().get().cloned()
    }

    /// Share immutable package data, keeping this generator's mutable caches.
    pub fn install_navigation_snapshot(&self, snapshot: Arc<NavigationSnapshot>) -> Result<()> {
        if snapshot.side != self.navigation_chunk_side() {
            return Err(EnvironmentPageError::Invalid);
        }
        self.static_navigation()
            .set(snapshot)
            .map_err(|_| EnvironmentPageError::Invalid)
    }

    /// Snapshot bytes/chunks; shared graph references are counted once here.
    pub fn static_navigation_usage(&self) -> (usize, usize) {
        self.static_navigation()
            .get()
            .map_or((0, 0), |data| (data.bytes, data.chunks.len()))
    }

    /// Memo entries in the current prepared collision atlas.
    pub fn prepared_navigation_entries(&self) -> usize {
        self.portal_cache()
            .lock()
            .ok()
            .and_then(|cache| {
                cache
                    .prepared
                    .as_ref()
                    .and_then(|atlas| atlas.entry_count())
            })
            .unwrap_or(0)
    }

    pub fn has_static_navigation(&self) -> bool {
        self.static_navigation().get().is_some()
    }

    pub(crate) fn static_navigation_chunk(&self, key: Key) -> Option<&NavigationChunk> {
        let data = self.static_navigation().get()?;
        if key.0 < 0 || key.1 < 0 || key.0 >= data.side || key.1 >= data.side {
            return None;
        }
        data.chunks.get((key.1 * data.side + key.0) as usize)
    }

    pub(crate) fn static_navigation_cell(&self, tile: TileCoord) -> Option<Option<WalkCell>> {
        self.static_navigation_chunk(chunk_of(tile))
            .map(|chunk| chunk.cell(grid::index(tile)))
    }

    pub fn install_static_navigation(&self, mut chunks: Vec<NavigationChunk>) -> Result<()> {
        let side = self.navigation_chunk_side();
        if side <= 0 || chunks.len() != side as usize * side as usize {
            return Err(EnvironmentPageError::Invalid);
        }
        chunks.sort_unstable_by_key(|chunk| (chunk.key.1, chunk.key.0));
        if chunks
            .iter()
            .enumerate()
            .any(|(at, chunk)| chunk.key != (at as i32 % side, at as i32 / side))
        {
            return Err(EnvironmentPageError::Invalid);
        }
        let bytes = chunks.iter().map(NavigationChunk::retained_bytes).sum();
        self.static_navigation()
            .set(Arc::new(NavigationSnapshot {
                side,
                chunks,
                bytes,
            }))
            .map_err(|_| EnvironmentPageError::Invalid)
    }

    pub fn prepare_static_navigation_chunk(
        &self,
        x: i32,
        y: i32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<NavigationChunk> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let side = self.navigation_chunk_side();
        if x < 0 || y < 0 || x >= side || y >= side {
            return Err(EnvironmentPageError::Invalid);
        }
        let cells = self.navigation_cells((x, y), cancelled)?;
        let mut runs: Vec<WalkRun> = Vec::new();
        let mut nodes = Vec::new();
        for (at, cell) in cells.into_iter().enumerate() {
            let (flags, height) = cell.map_or((0, 0), |cell| {
                if let Some(node) = cell.node {
                    nodes.push(PackedNode::new(at as u16, node));
                }
                (
                    1 | (u8::from(cell.passable) << 1)
                        | (u8::from(cell.walkable_surface) << 2)
                        | (u8::from(cell.mud) << 3),
                    cell.game_height_level,
                )
            });
            if let Some(last) = runs
                .last_mut()
                .filter(|last| last.flags == flags && last.height == height)
            {
                last.end = at as u16 + 1;
            } else {
                runs.push(WalkRun {
                    end: at as u16 + 1,
                    flags,
                    height,
                });
            }
        }
        let overlay = ResourceOverlay::default();
        let ground = Ground::new(self, &overlay, cancelled);
        let graph = Arc::new(ground.build_portals((x, y))?);
        if let Some(portal) = graph.portals.first() {
            ground.row((x, y), &graph, portal.tile)?;
        } else {
            let _ = graph.costs.set(Vec::new());
        }
        Ok(NavigationChunk {
            key: (x, y),
            runs: runs.into_boxed_slice(),
            nodes: nodes.into_boxed_slice(),
            graph,
        })
    }
}

/// Integrity of a persisted navigation chunk.
pub fn navigation_checksum(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

#[cfg(test)]
#[path = "tests/compact.rs"]
mod tests;
