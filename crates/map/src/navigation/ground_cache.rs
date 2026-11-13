//! Shared ship ground, computed once per visited tile and collision change.
//! Compact ground bits survive queries in other regions. Only the larger edge
//! tables evict; collisions invalidate local chunks without visiting units.
use super::tile_hash::TileHasher;
use aoe_core::{TileCoord, TileRect};
use std::{collections::HashMap, hash::BuildHasherDefault};

const SIDE: i32 = 32;
const SLOTS: usize = 4096;

#[derive(Debug, Default)]
struct GroundChunk {
    known: [u64; 16],
    wet: [u64; 16],
}

#[derive(Debug)]
struct EdgeChunk {
    key: (i32, i32),
    /// Low eight bits say which directed edges are known; high bits are clear.
    edges: [u16; 1024],
}

/// Derived ground/edge answers for one caller-owned movement domain.
/// Ground retention scales with visited terrain, never units or orders:
/// 256 bitmap bytes per visited 32-tile chunk, plus sparse index/allocation
/// overhead. The caller computes misses and invalidates changed ground.
#[derive(Debug, Default)]
pub struct RouteGroundCache {
    ground: HashMap<(i32, i32), Box<GroundChunk>, BuildHasherDefault<TileHasher>>,
    // Only edges evict: at most 4096 * (2056-byte chunk + pointer), excluding
    // allocator metadata. An evicted edge reuses the retained ground bits.
    edges: Vec<Option<Box<EdgeChunk>>>,
    misses: usize,
}

#[inline]
fn key(tile: TileCoord) -> (i32, i32) {
    (tile.x.div_euclid(SIDE), tile.y.div_euclid(SIDE))
}

#[inline]
fn slot((x, y): (i32, i32)) -> usize {
    let hash = (x as u32 as u64).wrapping_mul(0x9e37_79b1_85eb_ca87)
        ^ (y as u32 as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
    ((hash >> 32) as usize) & (SLOTS - 1)
}

#[inline]
fn bit(tile: TileCoord) -> (usize, u64) {
    let index = (tile.y.rem_euclid(SIDE) * SIDE + tile.x.rem_euclid(SIDE)) as usize;
    (index / 64, 1 << (index % 64))
}

#[inline]
fn direction(from: TileCoord, to: TileCoord) -> Option<u16> {
    match (to.x.checked_sub(from.x)?, to.y.checked_sub(from.y)?) {
        (-1, -1) => Some(1),
        (0, -1) => Some(2),
        (1, -1) => Some(4),
        (-1, 0) => Some(8),
        (1, 0) => Some(16),
        (-1, 1) => Some(32),
        (0, 1) => Some(64),
        (1, 1) => Some(128),
        _ => None,
    }
}

impl RouteGroundCache {
    /// Cumulative uncached tile queries; diagnostic only, saturating on overflow.
    pub fn miss_count(&self) -> usize {
        self.misses
    }

    #[inline]
    pub fn get(&mut self, tile: TileCoord) -> Option<bool> {
        let key = key(tile);
        let (word, mask) = bit(tile);
        let answer = self.ground.get(&key).and_then(|chunk| {
            (chunk.known[word] & mask != 0).then_some(chunk.wet[word] & mask != 0)
        });
        if answer.is_none() {
            self.misses = self.misses.saturating_add(1);
        }
        answer
    }

    fn edge_chunk_mut(&mut self, tile: TileCoord) -> &mut EdgeChunk {
        if self.edges.is_empty() {
            self.edges.resize_with(SLOTS, || None);
        }
        let key = key(tile);
        let entry = &mut self.edges[slot(key)];
        if entry.as_ref().is_some_and(|chunk| chunk.key != key) {
            *entry = None;
        }
        entry.get_or_insert_with(|| {
            Box::new(EdgeChunk {
                key,
                edges: [0; 1024],
            })
        })
    }

    #[inline]
    pub fn insert(&mut self, tile: TileCoord, wet: bool) {
        let chunk = self.ground.entry(key(tile)).or_default();
        let (word, mask) = bit(tile);
        chunk.known[word] |= mask;
        if wet {
            chunk.wet[word] |= mask;
        } else {
            chunk.wet[word] &= !mask;
        }
    }

    #[inline]
    pub fn get_edge(&self, from: TileCoord, to: TileCoord) -> Option<bool> {
        let mask = direction(from, to)?;
        let key = key(from);
        let chunk = self.edges.get(slot(key))?.as_ref()?;
        if chunk.key != key {
            return None;
        }
        let (word, bit) = bit(from);
        let index = word * 64 + bit.trailing_zeros() as usize;
        let edge = chunk.edges[index];
        (edge & mask != 0).then_some(edge & (mask << 8) != 0)
    }

    #[inline]
    pub fn insert_edge(&mut self, from: TileCoord, to: TileCoord, clear: bool) {
        let Some(mask) = direction(from, to) else {
            return;
        };
        let chunk = self.edge_chunk_mut(from);
        let (word, bit) = bit(from);
        let edge = &mut chunk.edges[word * 64 + bit.trailing_zeros() as usize];
        *edge |= mask;
        if clear {
            *edge |= mask << 8;
        } else {
            *edge &= !(mask << 8);
        }
    }

    pub fn invalidate(&mut self, area: TileRect) {
        if area.min.x >= area.max.x || area.min.y >= area.max.y {
            return;
        }
        // A directed diagonal also reads its two corners, at most one tile
        // from its source. Evict those sources even across chunk boundaries.
        let min = key(TileCoord::new(
            area.min.x.saturating_sub(1),
            area.min.y.saturating_sub(1),
        ));
        let max = key(TileCoord::new(area.max.x, area.max.y));
        let count =
            (i64::from(max.0) - i64::from(min.0) + 1) * (i64::from(max.1) - i64::from(min.1) + 1);
        if count > self.ground.len() as i64 {
            self.ground
                .retain(|&(x, y), _| x < min.0 || x > max.0 || y < min.1 || y > max.1);
        } else {
            for y in min.1..=max.1 {
                for x in min.0..=max.0 {
                    self.ground.remove(&(x, y));
                }
            }
        }
        if self.edges.is_empty() {
            return;
        }
        if count > SLOTS as i64 {
            for entry in &mut self.edges {
                if entry.as_ref().is_some_and(|chunk| {
                    chunk.key.0 >= min.0
                        && chunk.key.0 <= max.0
                        && chunk.key.1 >= min.1
                        && chunk.key.1 <= max.1
                }) {
                    *entry = None;
                }
            }
        } else {
            for y in min.1..=max.1 {
                for x in min.0..=max.0 {
                    let entry = &mut self.edges[slot((x, y))];
                    if entry.as_ref().is_some_and(|chunk| chunk.key == (x, y)) {
                        *entry = None;
                    }
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.ground.clear();
        self.edges.clear();
    }
}

#[cfg(test)]
mod tests;
