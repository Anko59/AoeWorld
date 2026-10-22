//! Resource nodes read from public map chunks, the same terrain the browser
//! downloads. A node is exposed to a bot only once its tile is explored.
use aoe_core::TileCoord;
use aoe_map::{CompactChunk, LandscapeChunk, ResourceKind as MapKind};
use aoe_protocol::ResourceKind;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Node {
    pub id: u64,
    pub tile: TileCoord,
    pub kind: ResourceKind,
    pub initial: u16,
}

#[derive(Clone, Debug, Default)]
pub struct ResourceCatalog {
    pub(crate) nodes: BTreeMap<u64, Node>,
    loaded: BTreeSet<(i32, i32)>,
    requested: BTreeSet<(i32, i32)>,
    /// 32x32 walkable flags per chunk (the map format's fixed chunk edge).
    passable: BTreeMap<(i32, i32), Box<[bool; 1024]>>,
}

impl ResourceCatalog {
    /// Chunks not yet requested, marked as requested (at most `limit`).
    pub(crate) fn take_wanted(&mut self, wanted: &[(i32, i32)], limit: usize) -> Vec<(i32, i32)> {
        let fresh: Vec<_> = wanted
            .iter()
            .filter(|chunk| !self.requested.contains(chunk))
            .take(limit)
            .copied()
            .collect();
        self.requested.extend(fresh.iter().copied());
        fresh
    }

    /// Allows a failed chunk to be requested again.
    pub(crate) fn forget(&mut self, chunk: (i32, i32)) {
        self.requested.remove(&chunk);
    }

    pub(crate) fn insert(&mut self, chunk: &CompactChunk) -> Result<(), String> {
        let landscape: LandscapeChunk = chunk.decode().map_err(|error| error.to_string())?;
        self.loaded.insert((landscape.x, landscape.y));
        let mut walkable = Box::new([false; 1024]);
        for tile in &landscape.tiles {
            walkable[(tile.tile.y.rem_euclid(32) * 32 + tile.tile.x.rem_euclid(32)) as usize] =
                tile.terrain.passable;
        }
        self.passable.insert((landscape.x, landscape.y), walkable);
        for resource in &landscape.resources {
            let node = resource.node;
            self.nodes.insert(
                node.id,
                Node {
                    id: node.id,
                    tile: node.tile,
                    kind: match node.kind {
                        MapKind::Food => ResourceKind::Food,
                        MapKind::Wood => ResourceKind::Wood,
                        MapKind::Gold => ResourceKind::Gold,
                        MapKind::Stone => ResourceKind::Stone,
                    },
                    initial: node.initial_amount,
                },
            );
        }
        Ok(())
    }

    pub fn loaded_chunks(&self) -> usize {
        self.loaded.len()
    }

    /// Whether ground units can walk on `tile`, if its chunk has been read.
    pub fn passable(&self, tile: TileCoord) -> Option<bool> {
        let chunk = self
            .passable
            .get(&(tile.x.div_euclid(32), tile.y.div_euclid(32)))?;
        Some(chunk[(tile.y.rem_euclid(32) * 32 + tile.x.rem_euclid(32)) as usize])
    }
}
