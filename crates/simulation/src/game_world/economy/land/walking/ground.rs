//! Reuse bulk region geometry across overlapping fields.
use super::ZoneCache;
use aoe_core::{TileCoord, TileRect};
use aoe_map::{CHUNK_TILES, MapRegionGround};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

const CHUNK_AREA: usize = (CHUNK_TILES * CHUNK_TILES) as usize;
const TERRAIN_CACHE_CHUNKS: usize = 4096;
type Key = (i32, i32);
type Steps = [Option<u8>; CHUNK_AREA];

#[derive(Debug)]
pub(super) struct Ground {
    chunks: BTreeMap<Key, (u64, Arc<Steps>)>,
    order: VecDeque<Key>,
    retained_chunks: usize,
}

impl Default for Ground {
    fn default() -> Self {
        Self {
            chunks: BTreeMap::new(),
            order: VecDeque::new(),
            retained_chunks: TERRAIN_CACHE_CHUNKS,
        }
    }
}

impl ZoneCache {
    pub(super) fn ground_chunk(
        &self,
        region: &MapRegionGround<'_>,
        key: Key,
        size: (i32, i32),
    ) -> Arc<Steps> {
        let (x, y) = (key.0 * CHUNK_TILES, key.1 * CHUNK_TILES);
        let halo = TileRect::from_xywh(x - 1, y - 1, CHUNK_TILES + 2, CHUNK_TILES + 2);
        let revision = self.bridge_revision(halo);
        let mut ground = self
            .ground
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some((previous, steps)) = ground.chunks.get(&key)
            && *previous == revision
        {
            return steps.clone();
        }
        let steps = Arc::new(region.territory_chunk(key, size));
        if region.error().is_some() {
            return steps;
        }
        if !ground.chunks.contains_key(&key) {
            if ground.chunks.len() == ground.retained_chunks
                && let Some(old) = ground.order.pop_front()
            {
                ground.chunks.remove(&old);
            }
            ground.order.push_back(key);
        }
        ground.chunks.insert(key, (revision, steps.clone()));
        steps
    }
}

pub(super) fn chunk_index(tile: TileCoord) -> usize {
    (tile.y.rem_euclid(CHUNK_TILES) * CHUNK_TILES + tile.x.rem_euclid(CHUNK_TILES)) as usize
}
