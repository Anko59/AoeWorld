//! Chunk-shared collision snapshots. Route searches pin collision without
//! retaining resource amounts or copying the growing history on refresh.
use super::ResourceOverlay;
use crate::{CHUNK_TILES, ResourceNode};
use aoe_core::TileCoord;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

mod occupied;
mod recent;
use occupied::Occupied;

type Key = (i32, i32);
const SHARDS: usize = 64;
type ChunkTable = BTreeMap<Key, Arc<CollisionChunk>>;
type Shard = Arc<ChunkTable>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CollisionChunk {
    stamp: [u8; 32],
    occupied: Occupied,
    depleted: BTreeSet<u64>,
    bridges: BTreeMap<TileCoord, u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CollisionSnapshot {
    shards: Arc<[Shard; SHARDS]>,
    stamp: [u8; 32],
    bridge_tiles: usize,
}

impl Default for CollisionSnapshot {
    fn default() -> Self {
        let empty = Arc::default();
        Self {
            shards: Arc::new(std::array::from_fn(|_| Arc::clone(&empty))),
            stamp: [0; 32],
            bridge_tiles: 0,
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

    #[inline(always)]
    fn blocks_tile(&self, tile: TileCoord) -> bool {
        let at = key(tile);
        recent::read(&self.shards[Self::shard(at)], at, |chunk| {
            chunk.is_some_and(|chunk| chunk.occupied.contains_key(&tile))
        })
    }

    #[inline(always)]
    fn blocks_node(&self, node: ResourceNode) -> bool {
        // Fish lie below a completed deck; water itself controls land access.
        !matches!(
            node.object,
            crate::ObjectKind::ShoreFish | crate::ObjectKind::DeepFish
        ) && {
            let at = key(node.tile);
            !recent::read(&self.shards[Self::shard(at)], at, |chunk| {
                chunk.is_some_and(|chunk| chunk.depleted.contains(&node.id))
            })
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CollisionState {
    // The mutable overlay alone needs the complete set for its capacity check.
    pub(crate) obstacles: BTreeSet<TileCoord>,
    snapshot: CollisionSnapshot,
}

impl ResourceOverlay {
    pub(super) fn bridge_tile_count(&self) -> usize {
        self.navigation_state.snapshot.bridge_tiles
    }
    pub(crate) fn bridge_in_chunk(&self, chunk: (i32, i32)) -> bool {
        self.navigation_state.snapshot.bridge_tiles > 0
            && self
                .navigation_state
                .snapshot
                .chunk(chunk)
                .is_some_and(|c| !c.bridges.is_empty())
    }
    /// Neighbour direction bits of a completed raster deck tile.
    pub fn bridge_connections(&self, tile: TileCoord) -> Option<u16> {
        if self.navigation_state.snapshot.bridge_tiles == 0 {
            return None;
        }
        self.navigation_state
            .snapshot
            .chunk(key(tile))?
            .bridges
            .get(&tile)
            .copied()
    }

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
        if domain == 4 {
            if snapshot
                .chunk(key(tile))
                .is_some_and(|chunk| chunk.bridges.contains_key(&tile))
            {
                snapshot.bridge_tiles -= 1;
            } else {
                snapshot.bridge_tiles += 1;
            }
        }
        let chunk = snapshot.chunk_mut(key(tile));
        if domain == 4 {
            if chunk.bridges.remove(&tile).is_none() {
                chunk.bridges.insert(tile, identity as u16);
            }
        } else if domain == 2 {
            chunk.depleted.insert(identity);
        } else {
            let flag = if domain == 1 { 1 } else { 2 };
            let present = chunk.occupied.toggle(tile, flag);
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
        let empty = chunk.stamp == [0; 32]
            && chunk.occupied.is_empty()
            && chunk.depleted.is_empty()
            && chunk.bridges.is_empty();
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
