//! Compact immutable navigation chunks; adapters own persistence and workers.
use super::packed::PackedChunk;
use super::*;

pub const NAVIGATION_FORMAT_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WalkRun {
    pub(super) end: u16,
    pub(super) flags: u8,
    pub(super) height: i16,
}

#[derive(Debug)]
pub(super) struct PackedNode {
    pub(super) id: u64,
    pub(super) at: u16,
    pub(super) amount: u16,
    pub(super) kind: crate::ResourceKind,
    pub(super) object: crate::ObjectKind,
    pub(super) variant: u8,
}
impl PackedNode {
    pub(super) fn new(at: u16, node: crate::ResourceNode) -> Self {
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
    pub(super) key: Key,
    pub(super) runs: Box<[WalkRun]>,
    pub(super) nodes: Box<[PackedNode]>,
    pub(super) graph: Arc<ChunkPortals>,
    pub(super) packed: Option<PackedChunk>,
}
impl NavigationChunk {
    /// Retained allocation capacities, excluding allocator bookkeeping.
    pub fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + self.runs.len() * size_of::<WalkRun>()
            + self.nodes.len() * size_of::<PackedNode>()
            + self.graph.bytes()
            + self.packed.as_ref().map_or(0, |p| p.storage.heap_bytes())
    }
    pub fn coordinates(&self) -> (i32, i32) {
        self.key
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        packed::encode(self)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        packed::decode(bytes)
    }
    pub(super) fn cell(&self, at: usize) -> Option<WalkCell> {
        if let Some(packed) = &self.packed {
            return packed.view().cell(at);
        }
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
        if graph.portal_count() > 0 {
            ground.ensure_costs((x, y), &graph)?;
        } else {
            let _ = graph.costs.set(Vec::new());
        }
        Ok(NavigationChunk {
            key: (x, y),
            runs: runs.into_boxed_slice(),
            nodes: nodes.into_boxed_slice(),
            graph,
            packed: None,
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
