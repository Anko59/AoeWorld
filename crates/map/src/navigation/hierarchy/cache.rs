//! Byte-bounded, collision-keyed portal cache.
use super::*;

/// Chunk portal lists keyed by chunk, bounded by bytes with least-recently-used
/// eviction. A cost matrix is computed lazily after insertion, so an entry is
/// charged for its full `portals x portals` matrix up front.
#[derive(Debug)]
pub(crate) struct PortalCache {
    pub(super) chunks: HashMap<Key, ([u8; 32], Arc<ChunkPortals>, u64)>,
    pub(super) prepared: Option<Arc<prepared::Atlas>>,
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
            prepared: None,
            bytes: 0,
            clock: 0,
            limit,
        }
    }

    pub(super) fn chunk(&mut self, key: Key, stamp: [u8; 32]) -> Option<Arc<ChunkPortals>> {
        self.clock += 1;
        let clock = self.clock;
        self.chunks
            .get_mut(&key)
            .filter(|(known, _, _)| *known == stamp)
            .map(|(_, found, touched)| {
                *touched = clock;
                Arc::clone(found)
            })
    }

    pub(crate) fn insert(&mut self, key: Key, stamp: [u8; 32], portals: Arc<ChunkPortals>) {
        self.clock += 1;
        self.bytes += portals.bytes();
        if let Some((_, old, _)) = self.chunks.insert(key, (stamp, portals, self.clock)) {
            self.bytes -= old.bytes();
        }
        if self.bytes > self.limit {
            let mut stamps: Vec<_> = self.chunks.iter().map(|(k, (_, _, s))| (*s, *k)).collect();
            stamps.sort_unstable();
            for (_, key) in stamps {
                if self.bytes <= self.limit / 10 * 9 {
                    break;
                }
                if let Some((_, old, _)) = self.chunks.remove(&key) {
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
