//! Packed static navigation chunks decoded once they are read often. A packed
//! chunk answers a tile with two binary searches and a node decode; a decoded
//! one with an index. Every decoded cell is the packed `cell` answer itself,
//! so answers never change, and a byte budget bounds what is kept.
use super::packed::View;
use crate::WalkCell;
use crate::{CHUNK_TILES, ResourceNode};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering::Relaxed};

const CELLS: usize = (CHUNK_TILES * CHUNK_TILES) as usize;
/// Reads of one chunk before it is decoded: decoding costs about one read
/// per tile, so it pays for itself within twice this many reads.
const DECODE_AFTER: u16 = 1_024;
/// Bytes of pages and decoded chunks (about 4 KiB each, plus nodes) one
/// snapshot may keep.
const BUDGET_BYTES: usize = 64 << 20;

const PRESENT: u32 = 1;
const PASSABLE: u32 = 2;
const WALKABLE: u32 = 4;
const MUD: u32 = 8;
const NODE: u32 = 16;

struct Chunk {
    /// Flags in the low bits, the height level in the high 16.
    cells: Box<[u32; CELLS]>,
    /// `(local index, node)` ordered by local index.
    nodes: Vec<(u16, ResourceNode)>,
}

impl Chunk {
    fn decode(view: View<'_>) -> Self {
        let mut cells = Box::new([0; CELLS]);
        let mut nodes = Vec::new();
        for (index, cell) in cells.iter_mut().enumerate() {
            let Some(walk) = view.cell(index) else {
                continue;
            };
            let flags = [
                (walk.passable, PASSABLE),
                (walk.walkable_surface, WALKABLE),
                (walk.mud, MUD),
                (walk.node.is_some(), NODE),
            ];
            let flags = flags
                .iter()
                .filter(|(on, _)| *on)
                .fold(PRESENT, |bits, (_, flag)| bits | flag);
            *cell = flags | (u32::from(walk.game_height_level as u16) << 16);
            if let Some(node) = walk.node {
                nodes.push((index as u16, node));
            }
        }
        Self { cells, nodes }
    }

    fn cell(&self, index: usize) -> Option<WalkCell> {
        let cell = self.cells[index];
        (cell & PRESENT != 0).then(|| WalkCell {
            passable: cell & PASSABLE != 0,
            walkable_surface: cell & WALKABLE != 0,
            mud: cell & MUD != 0,
            game_height_level: (cell >> 16) as u16 as i16,
            node: (cell & NODE != 0)
                .then(|| {
                    self.nodes
                        .binary_search_by_key(&(index as u16), |(at, _)| *at)
                        .ok()
                        .map(|at| self.nodes[at].1)
                })
                .flatten(),
        })
    }

    fn bytes(&self) -> usize {
        size_of::<Self>() + CELLS * 4 + self.nodes.capacity() * size_of::<(u16, ResourceNode)>()
    }
}

/// Chunk slots allocated together, only once one of them is read.
const PAGE: usize = 256;

struct Page {
    reads: [AtomicU16; PAGE],
    chunks: [OnceLock<Chunk>; PAGE],
}

/// Decoded chunks of one snapshot. Nothing is allocated until a chunk is
/// read, so a snapshot keeps its small offset index until play starts.
pub(super) struct Decoded {
    count: usize,
    pages: OnceLock<Box<[OnceLock<Box<Page>>]>>,
    bytes: AtomicUsize,
}

impl std::fmt::Debug for Decoded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decoded")
            .field("count", &self.count)
            .field("bytes", &self.bytes.load(Relaxed))
            .finish_non_exhaustive()
    }
}

impl Decoded {
    pub(super) fn new(count: usize) -> Self {
        Self {
            count,
            pages: OnceLock::new(),
            bytes: AtomicUsize::new(0),
        }
    }

    fn page(&self, at: usize) -> &Page {
        let pages = self.pages.get_or_init(|| {
            let pages = self.count.div_ceil(PAGE);
            self.bytes
                .fetch_add(pages * size_of::<OnceLock<Box<Page>>>(), Relaxed);
            (0..pages).map(|_| OnceLock::new()).collect()
        });
        pages[at / PAGE].get_or_init(|| {
            self.bytes.fetch_add(size_of::<Page>(), Relaxed);
            Box::new(Page {
                reads: std::array::from_fn(|_| AtomicU16::new(0)),
                chunks: std::array::from_fn(|_| OnceLock::new()),
            })
        })
    }

    /// The cell at `index` of chunk `at`, decoding the chunk once it has been
    /// read often and the budget allows; otherwise straight from `view`.
    pub(super) fn cell<'a>(
        &self,
        at: usize,
        view: impl FnOnce() -> View<'a>,
        index: usize,
    ) -> Option<WalkCell> {
        if at >= self.count {
            return view().cell(index);
        }
        let page = self.page(at);
        let (slot, reads) = (&page.chunks[at % PAGE], &page.reads[at % PAGE]);
        if let Some(chunk) = slot.get() {
            return chunk.cell(index);
        }
        // A plain load and store: a lost increment only delays the decode.
        let n = reads.load(Relaxed);
        if n < DECODE_AFTER || self.bytes.load(Relaxed) >= BUDGET_BYTES {
            reads.store(n.saturating_add(1).min(DECODE_AFTER), Relaxed);
            return view().cell(index);
        }
        slot.get_or_init(|| {
            let chunk = Chunk::decode(view());
            self.bytes.fetch_add(chunk.bytes(), Relaxed);
            chunk
        })
        .cell(index)
    }

    /// Pages and decoded chunks kept so far.
    pub(super) fn heap_bytes(&self) -> usize {
        self.bytes.load(Relaxed)
    }
}
