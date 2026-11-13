//! Sparse occupancy membership, updated on placement changes, never iterated.
use crate::game_world::visibility::TileHasher;
use aoe_core::TileCoord;
use aoe_map::CHUNK_TILES;
use std::{collections::HashMap, hash::BuildHasherDefault};

const WORDS: usize = (CHUNK_TILES * CHUNK_TILES / 64) as usize;

#[derive(Debug, Default)]
struct Chunk {
    bits: [u64; WORDS],
    count: u16,
}

/// One bit per blocked tile in each occupied chunk, plus sparse index overhead.
/// Empty queries allocate nothing; removing a chunk's final tile releases it.
#[derive(Debug, Default)]
pub(crate) struct BlockedTiles {
    chunks: HashMap<TileCoord, Box<Chunk>, BuildHasherDefault<TileHasher>>,
    len: usize,
}

#[inline]
fn address(tile: TileCoord) -> (TileCoord, usize, u64) {
    let key = TileCoord::new(
        tile.x.div_euclid(CHUNK_TILES),
        tile.y.div_euclid(CHUNK_TILES),
    );
    let index =
        (tile.y.rem_euclid(CHUNK_TILES) * CHUNK_TILES + tile.x.rem_euclid(CHUNK_TILES)) as usize;
    (key, index / 64, 1_u64 << (index % 64))
}

impl BlockedTiles {
    #[inline]
    pub(crate) fn contains(&self, tile: &TileCoord) -> bool {
        if self.is_empty() {
            return false;
        }
        let (key, word, mask) = address(*tile);
        self.chunks
            .get(&key)
            .is_some_and(|chunk| chunk.bits[word] & mask != 0)
    }

    pub(crate) fn insert(&mut self, tile: TileCoord) -> bool {
        let (key, word, mask) = address(tile);
        let chunk = self.chunks.entry(key).or_default();
        if chunk.bits[word] & mask != 0 {
            return false;
        }
        chunk.bits[word] |= mask;
        chunk.count += 1;
        self.len += 1;
        true
    }

    pub(crate) fn remove(&mut self, tile: &TileCoord) -> bool {
        let (key, word, mask) = address(*tile);
        let Some(chunk) = self.chunks.get_mut(&key) else {
            return false;
        };
        if chunk.bits[word] & mask == 0 {
            return false;
        }
        chunk.bits[word] &= !mask;
        chunk.count -= 1;
        self.len -= 1;
        if chunk.count == 0 {
            self.chunks.remove(&key);
        }
        true
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[cfg(test)]
    pub(super) fn resident_chunks(&self) -> usize {
        self.chunks.len()
    }
}

impl Extend<TileCoord> for BlockedTiles {
    fn extend<T: IntoIterator<Item = TileCoord>>(&mut self, tiles: T) {
        for tile in tiles {
            self.insert(tile);
        }
    }
}
