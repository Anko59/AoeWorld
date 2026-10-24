//! Chunk-shared collision snapshots. Route searches pin collision without
//! retaining resource amounts or copying the growing history on refresh.
use super::ResourceOverlay;
use crate::{CHUNK_TILES, ResourceNode};
use aoe_core::TileCoord;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

type Key = (i32, i32);
const SHARDS: usize = 64;
type Shard = Arc<BTreeMap<Key, Arc<CollisionChunk>>>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CollisionChunk {
    stamp: [u8; 32],
    occupied: BTreeMap<TileCoord, u8>,
    depleted: BTreeSet<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CollisionSnapshot {
    shards: Arc<[Shard; SHARDS]>,
    stamp: [u8; 32],
}

impl Default for CollisionSnapshot {
    fn default() -> Self {
        let empty = Arc::default();
        Self {
            shards: Arc::new(std::array::from_fn(|_| Arc::clone(&empty))),
            stamp: [0; 32],
        }
    }
}

fn key(tile: TileCoord) -> Key {
    (
        tile.x.div_euclid(CHUNK_TILES),
        tile.y.div_euclid(CHUNK_TILES),
    )
}

impl CollisionSnapshot {
    fn shard(key: Key) -> usize {
        ((key.0 as u32).wrapping_mul(31) ^ key.1 as u32) as usize % SHARDS
    }

    fn chunk(&self, key: Key) -> Option<&Arc<CollisionChunk>> {
        self.shards[Self::shard(key)].get(&key)
    }

    fn chunk_mut(&mut self, key: Key) -> &mut CollisionChunk {
        let shards = Arc::make_mut(&mut self.shards);
        let shard = Arc::make_mut(&mut shards[Self::shard(key)]);
        Arc::make_mut(shard.entry(key).or_default())
    }

    fn blocks_tile(&self, tile: TileCoord) -> bool {
        self.chunk(key(tile))
            .is_some_and(|chunk| chunk.occupied.contains_key(&tile))
    }

    fn blocks_node(&self, node: ResourceNode) -> bool {
        !self
            .chunk(key(node.tile))
            .is_some_and(|chunk| chunk.depleted.contains(&node.id))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CollisionState {
    // The mutable overlay alone needs the complete set for its capacity check.
    pub(crate) obstacles: BTreeSet<TileCoord>,
    snapshot: CollisionSnapshot,
}

impl ResourceOverlay {
    /// Capture collision only, sharing its table independently of resource
    /// history. Later writes copy the fixed table, one shard and one chunk.
    pub(crate) fn navigation_snapshot(&self) -> Self {
        Self {
            remaining: BTreeMap::new(),
            revision: self.revision,
            navigation_state: Box::new(CollisionState {
                obstacles: BTreeSet::new(),
                snapshot: self.navigation_state.snapshot.clone(),
            }),
        }
    }

    pub(crate) fn collision_blocks_tile(&self, tile: TileCoord) -> bool {
        self.navigation_state.snapshot.blocks_tile(tile)
    }

    pub(crate) fn collision_blocks_node(&self, node: ResourceNode) -> bool {
        self.navigation_state.snapshot.blocks_node(node)
    }

    pub(crate) fn navigation_stamp(&self) -> [u8; 32] {
        self.navigation_state.snapshot.stamp
    }

    pub(crate) fn navigation_toggle(&mut self, tile: TileCoord, domain: u8, identity: u64) {
        let mut hash = blake3::Hasher::new();
        hash.update(&[domain]);
        hash.update(&identity.to_le_bytes());
        hash.update(&tile.x.to_le_bytes());
        hash.update(&tile.y.to_le_bytes());
        let change = hash.finalize();
        let snapshot = &mut self.navigation_state.snapshot;
        let chunk = snapshot.chunk_mut(key(tile));
        if domain == 2 {
            chunk.depleted.insert(identity);
        } else {
            let flag = if domain == 1 { 1 } else { 2 };
            let occupied = chunk.occupied.entry(tile).or_default();
            *occupied ^= flag;
            let present = *occupied & flag != 0;
            if *occupied == 0 {
                chunk.occupied.remove(&tile);
            }
            if domain == 3 {
                if present {
                    chunk.depleted.remove(&identity);
                } else {
                    chunk.depleted.insert(identity);
                }
            }
        }
        for (local, byte) in chunk.stamp.iter_mut().zip(change.as_bytes()) {
            *local ^= byte;
        }
        let empty =
            chunk.stamp == [0; 32] && chunk.occupied.is_empty() && chunk.depleted.is_empty();
        for (global, byte) in snapshot.stamp.iter_mut().zip(change.as_bytes()) {
            *global ^= byte;
        }
        if empty {
            let shards = Arc::make_mut(&mut snapshot.shards);
            Arc::make_mut(&mut shards[CollisionSnapshot::shard(key(tile))]).remove(&key(tile));
        }
    }

    pub(crate) fn navigation_key(&self, key: Key) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        // Portals read both sides of each border.
        for (dx, dy) in [
            (0, 0),
            (0, -1),
            (-1, 0),
            (1, 0),
            (0, 1),
            (-1, -1),
            (-1, 1),
            (1, -1),
            (1, 1),
        ] {
            hash.update(
                &self
                    .navigation_state
                    .snapshot
                    .chunk((key.0 + dx, key.1 + dy))
                    .map_or([0; 32], |chunk| chunk.stamp),
            );
        }
        *hash.finalize().as_bytes()
    }

    pub(crate) fn rebuild_navigation(&mut self) {
        self.navigation_state.snapshot = CollisionSnapshot::default();
        let changes = self
            .remaining
            .iter()
            .map(|(id, amount)| (*id, *amount))
            .collect::<Vec<_>>();
        for (id, amount) in changes {
            let starting = ResourceNode::starting_from_id(id).is_some();
            let tile = TileCoord::new(
                ((id >> 1) & 0x3_ffff) as i32,
                ((id >> 19) & 0x3_ffff) as i32,
            );
            if (starting && amount > 0) || (!starting && amount == 0) {
                self.navigation_toggle(tile, if starting { 3 } else { 2 }, id);
            } else if starting && amount == 0 {
                self.navigation_state
                    .snapshot
                    .chunk_mut(key(tile))
                    .depleted
                    .insert(id);
            }
        }
    }
}

#[cfg(test)]
mod tests;
