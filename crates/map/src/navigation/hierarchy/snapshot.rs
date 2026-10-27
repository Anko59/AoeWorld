use super::packed::{NavigationStorage, PackedChunk, View};
use super::*;
use std::ops::Range;

#[derive(Debug)]
enum Chunks {
    Owned(Vec<NavigationChunk>),
    Packed {
        storage: Arc<dyn NavigationStorage>,
        records: Vec<Range<usize>>,
    },
}

#[derive(Debug)]
pub struct NavigationSnapshot {
    side: i32,
    chunks: Chunks,
    bytes: usize,
}
impl NavigationSnapshot {
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }
    pub fn heap_bytes(&self) -> usize {
        size_of::<Self>()
            + match &self.chunks {
                Chunks::Owned(chunks) => chunks.iter().map(NavigationChunk::retained_bytes).sum(),
                Chunks::Packed { storage, records } => {
                    storage.heap_bytes() + records.capacity() * size_of::<Range<usize>>()
                }
            }
    }
    pub fn chunk_count(&self) -> usize {
        match &self.chunks {
            Chunks::Owned(chunks) => chunks.len(),
            Chunks::Packed { records, .. } => records.len(),
        }
    }
    pub(crate) fn chunk_side(&self) -> i32 {
        self.side
    }
    pub fn from_storage(
        side: i32,
        storage: Arc<dyn NavigationStorage>,
        records: Vec<Range<usize>>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self> {
        if side <= 0
            || side as u64 > crate::request::MAX_TILES_PER_SIDE.div_ceil(CHUNK_TILES as u64)
            || records.len() != side as usize * side as usize
        {
            return Err(EnvironmentPageError::Invalid);
        }
        for (at, range) in records.iter().enumerate() {
            if cancelled() {
                return Err(EnvironmentPageError::Cancelled);
            }
            let bytes = storage
                .bytes()
                .get(range.clone())
                .ok_or(EnvironmentPageError::Invalid)?;
            let view = View::validate(bytes)?;
            if view.key() != (at as i32 % side, at as i32 / side) {
                return Err(EnvironmentPageError::Invalid);
            }
        }
        let bytes = size_of::<Self>()
            + storage.bytes().len().max(storage.heap_bytes())
            + records.capacity() * size_of::<Range<usize>>();
        Ok(Self {
            side,
            chunks: Chunks::Packed { storage, records },
            bytes,
        })
    }
    fn index(&self, key: Key) -> Option<usize> {
        (key.0 >= 0 && key.1 >= 0 && key.0 < self.side && key.1 < self.side)
            .then_some((key.1 as i64 * self.side as i64 + key.0 as i64) as usize)
    }
    fn cell(&self, tile: TileCoord) -> Option<Option<WalkCell>> {
        let at = self.index(chunk_of(tile))?;
        Some(match &self.chunks {
            Chunks::Owned(chunks) => chunks[at].cell(grid::index(tile)),
            Chunks::Packed { storage, records } => {
                View::new(&storage.bytes()[records[at].clone()]).cell(grid::index(tile))
            }
        })
    }
    fn graph(&self, key: Key) -> Option<Arc<ChunkPortals>> {
        let at = self.index(key)?;
        Some(match &self.chunks {
            Chunks::Owned(chunks) => Arc::clone(&chunks[at].graph),
            Chunks::Packed { storage, records } => {
                Arc::new(ChunkPortals::from_packed(PackedChunk {
                    storage: storage.clone(),
                    range: records[at].clone(),
                }))
            }
        })
    }
}

impl MapChunkGenerator {
    pub fn navigation_snapshot(&self) -> Option<Arc<NavigationSnapshot>> {
        self.static_navigation().get().cloned()
    }
    pub fn install_navigation_snapshot(&self, snapshot: Arc<NavigationSnapshot>) -> Result<()> {
        if snapshot.side != self.navigation_chunk_side() {
            return Err(EnvironmentPageError::Invalid);
        }
        self.static_navigation()
            .set(snapshot)
            .map_err(|_| EnvironmentPageError::Invalid)
    }
    pub fn static_navigation_usage(&self) -> (usize, usize) {
        self.static_navigation()
            .get()
            .map_or((0, 0), |data| (data.bytes, data.chunk_count()))
    }
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
    pub(crate) fn static_navigation_graph(&self, key: Key) -> Option<Arc<ChunkPortals>> {
        self.static_navigation().get()?.graph(key)
    }
    pub(crate) fn static_navigation_cell(&self, tile: TileCoord) -> Option<Option<WalkCell>> {
        self.static_navigation().get()?.cell(tile)
    }
    /// In-memory preparation remains useful to pure adapters and small tests.
    /// Native package baking streams to storage and installs byte views instead.
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
        let bytes = size_of::<NavigationSnapshot>()
            + chunks
                .iter()
                .map(NavigationChunk::retained_bytes)
                .sum::<usize>();
        self.install_navigation_snapshot(Arc::new(NavigationSnapshot {
            side,
            chunks: Chunks::Owned(chunks),
            bytes,
        }))
    }
}
