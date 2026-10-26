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
    bridges: BTreeMap<TileCoord, bool>,
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

    fn blocks_tile(&self, tile: TileCoord) -> bool {
        self.chunk(key(tile))
            .is_some_and(|chunk| chunk.occupied.contains_key(&tile))
    }

    fn blocks_node(&self, node: ResourceNode) -> bool {
        // Fish lie below a completed deck; water itself controls land access.
        !matches!(
            node.object,
            crate::ObjectKind::ShoreFish | crate::ObjectKind::DeepFish
        ) && !self
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
    /// Axis of a completed deck tile: true for the world X axis.
    pub fn bridge_axis(&self, tile: TileCoord) -> Option<bool> {
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

    /// Overrides terrain edges touching a bridge. Banks are ordinary land;
    /// only the water deck is restricted, preventing lateral/diagonal entry.
    pub fn bridge_edge(&self, from: TileCoord, to: TileCoord) -> Option<bool> {
        let a = self.bridge_axis(from);
        let b = self.bridge_axis(to);
        let axis = a.or(b)?;
        Some(
            a.zip(b).is_none_or(|(a, b)| a == b)
                && if axis {
                    from.y == to.y && from.x.abs_diff(to.x) == 1
                } else {
                    from.x == to.x && from.y.abs_diff(to.y) == 1
                },
        )
    }

    /// Full one-tile-wide footprint, including the two land endpoints.
    /// Only changed chunks and their border neighbours acquire new stamps.
    pub fn add_bridge(
        &mut self,
        rect: aoe_core::TileRect,
    ) -> Result<(), super::ResourceOverlayError> {
        let deck = bridge_deck(rect).ok_or(super::ResourceOverlayError::InvalidSnapshot)?;
        let count = u64::try_from(deck.width().max(deck.height())).unwrap_or(u64::MAX);
        if (self.bridge_tile_count() as u64)
            .saturating_add(self.navigation_state.obstacles.len() as u64)
            .saturating_add(count)
            > super::MAX_OBSTACLE_TILES
        {
            return Err(super::ResourceOverlayError::Capacity);
        }
        for y in deck.min.y..deck.max.y {
            for x in deck.min.x..deck.max.x {
                if self.bridge_axis(TileCoord::new(x, y)).is_some() {
                    return Err(super::ResourceOverlayError::InvalidSnapshot);
                }
            }
        }
        self.toggle_bridge(deck, rect.height() == 1);
        Ok(())
    }

    /// Atomically replace a reserved foundation with its deck. Reserving the
    /// full span already accounts for more cells than the completed deck.
    pub fn complete_bridge(&mut self, rect: aoe_core::TileRect) -> bool {
        let Some(deck) = bridge_deck(rect) else {
            return false;
        };
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                let tile = TileCoord::new(x, y);
                if !self.navigation_state.obstacles.contains(&tile)
                    || self.bridge_axis(tile).is_some()
                {
                    return false;
                }
            }
        }
        self.remove_footprint(rect);
        self.toggle_bridge(deck, rect.height() == 1);
        true
    }

    pub fn remove_bridge(&mut self, rect: aoe_core::TileRect) {
        if let Some(deck) = bridge_deck(rect) {
            let axis = rect.height() == 1;
            for y in deck.min.y..deck.max.y {
                for x in deck.min.x..deck.max.x {
                    let tile = TileCoord::new(x, y);
                    if self.bridge_axis(tile) == Some(axis) {
                        self.navigation_toggle(tile, if axis { 4 } else { 5 }, 0);
                    }
                }
            }
        }
    }

    fn toggle_bridge(&mut self, deck: aoe_core::TileRect, axis: bool) {
        for y in deck.min.y..deck.max.y {
            for x in deck.min.x..deck.max.x {
                self.navigation_toggle(TileCoord::new(x, y), if axis { 4 } else { 5 }, 0);
            }
        }
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
        if domain == 4 || domain == 5 {
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
        if domain == 4 || domain == 5 {
            if chunk.bridges.remove(&tile).is_none() {
                chunk.bridges.insert(tile, domain == 4);
            }
        } else if domain == 2 {
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

fn bridge_deck(rect: aoe_core::TileRect) -> Option<aoe_core::TileRect> {
    if rect.min.x < 0 || rect.min.y < 0 {
        return None;
    }
    let mut deck = rect;
    if rect.height() == 1 && (3..=42).contains(&rect.width()) {
        deck.min.x += 1;
        deck.max.x -= 1;
    } else if rect.width() == 1 && (3..=42).contains(&rect.height()) {
        deck.min.y += 1;
        deck.max.y -= 1;
    } else {
        return None;
    }
    Some(deck)
}

#[cfg(test)]
mod tests;
