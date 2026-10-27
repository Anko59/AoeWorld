//! Resource nodes read from public map chunks, the same terrain the browser
//! downloads. A node is exposed to a bot only once its tile is explored.
use aoe_core::TileCoord;
use aoe_map::{CompactChunk, LandscapeChunk, ResourceKind as MapKind, WaterKind};
use aoe_protocol::ResourceKind;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Node {
    pub id: u64,
    pub tile: TileCoord,
    pub kind: ResourceKind,
    pub initial: u16,
}

/// The first resource id of the starting kit (bit 37, see `aoe_map::ResourceNode::starting`).
pub(crate) const STARTING_ID_BASE: u64 = 1 << 37;

impl Node {
    /// A starting-kit node (berries, gold, stone, trees placed beside a village)
    /// is not in any map chunk: its id alone says what and where it is.
    pub(crate) fn from_starting_id(id: u64) -> Option<Self> {
        let node = aoe_map::ResourceNode::starting_from_id(id)?;
        Some(Self {
            id,
            tile: node.tile,
            kind: map_kind(node.kind),
            initial: node.initial_amount,
        })
    }
}

fn map_kind(kind: MapKind) -> ResourceKind {
    match kind {
        MapKind::Food => ResourceKind::Food,
        MapKind::Wood => ResourceKind::Wood,
        MapKind::Gold => ResourceKind::Gold,
        MapKind::Stone => ResourceKind::Stone,
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Ground {
    passable: bool,
    height: i16,
    water: bool,
    dry: bool,
    navigable: bool,
}
type GroundChunk = Box<[Ground; 1024]>;

#[derive(Clone, Debug, Default)]
pub struct ResourceCatalog {
    pub(crate) nodes: BTreeMap<u64, Node>,
    loaded: BTreeSet<(i32, i32)>,
    requested: BTreeSet<(i32, i32)>,
    /// 32x32 walkable flags per chunk (the map format's fixed chunk edge).
    passable: BTreeMap<(i32, i32), GroundChunk>,
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
        let mut walkable = Box::new([Ground::default(); 1024]);
        for tile in &landscape.tiles {
            walkable[(tile.tile.y.rem_euclid(32) * 32 + tile.tile.x.rem_euclid(32)) as usize] =
                Ground {
                    passable: tile.terrain.passable && tile.terrain.surface.walkable(),
                    height: tile.terrain.game_height_level,
                    water: tile.terrain.water != WaterKind::None,
                    dry: tile.terrain.water == WaterKind::None && tile.terrain.passable,
                    navigable: tile.terrain.water != WaterKind::None,
                };
        }
        self.passable.insert((landscape.x, landscape.y), walkable);
        for resource in &landscape.resources {
            let node = resource.node;
            self.nodes.insert(
                node.id,
                Node {
                    id: node.id,
                    tile: node.tile,
                    kind: map_kind(node.kind),
                    initial: node.initial_amount,
                },
            );
        }
        Ok(())
    }

    /// The public physical step rule, including cliff ledges between tiles.
    pub(crate) fn land_edge(&self, from: TileCoord, to: TileCoord) -> bool {
        let sample = |tile: TileCoord| {
            let chunk = self
                .passable
                .get(&(tile.x.div_euclid(32), tile.y.div_euclid(32)))?;
            Some(chunk[(tile.y.rem_euclid(32) * 32 + tile.x.rem_euclid(32)) as usize])
        };
        match (sample(from), sample(to)) {
            (Some(a), Some(b)) if a.passable && b.passable => {
                (i32::from(a.height) - i32::from(b.height)).abs() <= 1
            }
            _ => false,
        }
    }

    pub(crate) fn water(&self, tile: TileCoord) -> Option<bool> {
        let chunk = self
            .passable
            .get(&(tile.x.div_euclid(32), tile.y.div_euclid(32)))?;
        Some(chunk[(tile.y.rem_euclid(32) * 32 + tile.x.rem_euclid(32)) as usize].water)
    }

    pub fn loaded_chunks(&self) -> usize {
        self.loaded.len()
    }

    /// Whether ground units can walk on `tile`, if its chunk has been read.
    pub fn passable(&self, tile: TileCoord) -> Option<bool> {
        let chunk = self
            .passable
            .get(&(tile.x.div_euclid(32), tile.y.div_euclid(32)))?;
        Some(chunk[(tile.y.rem_euclid(32) * 32 + tile.x.rem_euclid(32)) as usize].passable)
    }

    pub(crate) fn dock_terrain(&self, tile: TileCoord) -> Option<(bool, bool)> {
        let chunk = self
            .passable
            .get(&(tile.x.div_euclid(32), tile.y.div_euclid(32)))?;
        let ground = chunk[(tile.y.rem_euclid(32) * 32 + tile.x.rem_euclid(32)) as usize];
        Some((ground.dry, ground.navigable))
    }
}
