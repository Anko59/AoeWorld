//! Fine collision cells complement the wildlife index. Normal cells are exact;
//! overfull spawn cells admit outward escape, but never new arrivals.
use crate::{GameWorld, Terrain, UnitKind};
use aoe_core::{EntityId, TileCoord, WorldPosition};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

const CELL_LIMIT: usize = 32;
type Cells = HashMap<u64, Vec<EntityId>, BuildHasherDefault<CellHasher>>;

// Coordinates are internal integer keys. Mix the packed pair once instead of
// running the general-purpose keyed byte hasher on every neighbour probe.
#[derive(Debug, Default)]
struct CellHasher(u64);
impl Hasher for CellHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = value.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(23);
    }
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
    }
}

fn key(tile: TileCoord) -> u64 {
    u64::from(tile.x as u32) | (u64::from(tile.y as u32) << 32)
}

#[derive(Debug, Default)]
pub(crate) struct CollisionIndex {
    cells: [Cells; 2],
    slots: Vec<usize>,
    indices: Vec<usize>,
    recycled: Vec<Vec<EntityId>>,
}

impl CollisionIndex {
    pub(crate) fn unit_index(&self, id: EntityId) -> usize {
        self.indices[id.0 as usize]
    }

    pub(crate) fn reindex(&mut self, id: EntityId, index: usize) {
        if let Some(slot) = self.indices.get_mut(id.0 as usize) {
            *slot = index;
        }
    }

    pub(crate) fn sampling_slot(&self, id: EntityId) -> u64 {
        self.slots
            .get(id.0 as usize)
            .filter(|&&n| n != usize::MAX)
            .map_or(u64::MAX, |&n| n as u64)
    }

    pub(crate) fn insert(&mut self, id: EntityId, tile: TileCoord, naval: bool, index: usize) {
        let at = id.0 as usize;
        if self.slots.get(at).is_some_and(|&slot| slot != usize::MAX) {
            return;
        }
        if at >= self.slots.len() {
            self.slots.resize(at + 1, usize::MAX);
            self.indices.resize(at + 1, usize::MAX);
        }
        self.indices[at] = index;
        let recycled = &mut self.recycled;
        let cell = self.cells[usize::from(naval)]
            .entry(key(tile))
            .or_insert_with(|| recycled.pop().unwrap_or_default());
        self.slots[at] = cell.len();
        cell.push(id);
    }

    pub(crate) fn remove(&mut self, id: EntityId, tile: TileCoord, naval: bool) {
        let Some(&slot) = self
            .slots
            .get(id.0 as usize)
            .filter(|&&slot| slot != usize::MAX)
        else {
            return;
        };
        let key = key(tile);
        let cells = &mut self.cells[usize::from(naval)];
        let Some(cell) = cells.get_mut(&key) else {
            return;
        };
        if cell.get(slot) != Some(&id) {
            return;
        }
        cell.swap_remove(slot);
        self.slots[id.0 as usize] = usize::MAX;
        if let Some(displaced) = cell.get(slot) {
            self.slots[displaced.0 as usize] = slot;
        }
        if cell.is_empty()
            && let Some(cell) = cells.remove(&key)
            && self.recycled.len() < 4096
        {
            self.recycled.push(cell);
        }
    }

    pub(crate) fn relocate(&mut self, id: EntityId, from: TileCoord, to: TileCoord, naval: bool) {
        if from != to {
            let index = self.unit_index(id);
            self.remove(id, from, naval);
            self.insert(id, to, naval, index);
        }
    }

    /// At most sixteen cells, with 32 body tests per cell. Hash iteration never
    /// determines a decision: cells and each cell's cyclic window are canonical.
    fn blocked(
        &self,
        at: WorldPosition,
        reach: i32,
        naval: bool,
        salt: u64,
        mut test: impl FnMut(EntityId) -> bool,
        mut overflow: impl FnMut(TileCoord) -> bool,
    ) -> bool {
        let min =
            WorldPosition::new(at.x.saturating_sub(reach), at.y.saturating_sub(reach)).tile_floor();
        let max =
            WorldPosition::new(at.x.saturating_add(reach), at.y.saturating_add(reach)).tile_floor();
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let tile = TileCoord::new(x, y);
                let Some(cell) = self.cells[usize::from(naval)].get(&key(tile)) else {
                    continue;
                };
                if cell.len() > CELL_LIMIT && overflow(tile) {
                    return true;
                }
                let start = (salt % cell.len() as u64) as usize;
                for n in 0..cell.len().min(CELL_LIMIT) {
                    if test(cell[(start + n) % cell.len()]) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

impl UnitKind {
    /// Equal collision axes in the complete classic DAT table, rounded up to
    /// fixed subunits. See docs/movement/aoe2-model.md for source and confidence.
    pub const fn collision_radius(self) -> i32 {
        match self {
            Self::Scout | Self::Knight | Self::LightCavalry => 256,
            Self::Vassal => 308,
            Self::BatteringRam => 461,
            Self::FishingShip
            | Self::Galley
            | Self::FireShip
            | Self::DemolitionShip
            | Self::TransportShip
            | Self::WarGalley
            | Self::Mangonel => 512,
            _ => 205,
        }
    }
}

impl GameWorld {
    pub(super) fn collision_occupied(
        &self,
        index: usize,
        at: WorldPosition,
        stationary: bool,
    ) -> bool {
        let unit = self.units[index].state;
        self.collisions.blocked(
            at,
            unit.kind.collision_radius() + 512,
            unit.kind.is_naval(),
            self.tick.0.wrapping_add(u64::from(unit.id.0)),
            |id| {
                if id == unit.id {
                    return false;
                }
                let other = self.units[self.collisions.unit_index(id)].state;
                let r = i64::from(unit.kind.collision_radius() + other.kind.collision_radius());
                let x = i64::from(at.x) - i64::from(other.position.x);
                let y = i64::from(at.y) - i64::from(other.position.y);
                other.hp > 0 && (!stationary || !other.moving) && x * x + y * y < r * r
            },
            |_| true,
        )
    }

    pub(in crate::game_world) fn collision_sweep(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        stationary: bool,
        repairing_spawn: bool,
    ) -> bool {
        let unit = self.units[index].state;
        let dx = i64::from(to.x) - i64::from(from.x);
        let dy = i64::from(to.y) - i64::from(from.y);
        let length = dx * dx + dy * dy;
        if length == 0 {
            return true;
        }
        let midpoint = WorldPosition::new(
            ((i64::from(from.x) + i64::from(to.x)) / 2) as i32,
            ((i64::from(from.y) + i64::from(to.y)) / 2) as i32,
        );
        !self.collisions.blocked(
            midpoint,
            unit.kind.collision_radius() + 768,
            unit.kind.is_naval(),
            self.tick.0.wrapping_add(u64::from(unit.id.0)),
            |id| {
                if id == unit.id {
                    return false;
                }
                let other = self.units[self.collisions.unit_index(id)].state;
                if other.hp == 0 || (stationary && other.moving) {
                    return false;
                }
                let r = i64::from(unit.kind.collision_radius() + other.kind.collision_radius());
                let x = i64::from(other.position.x) - i64::from(from.x);
                let y = i64::from(other.position.y) - i64::from(from.y);
                let start = x * x + y * y;
                let dot = x * dx + y * dy;
                // Existing overlaps may separate; priority never permits tunnelling
                // through a body that is already separated from the mover.
                if start < r * r && (repairing_spawn || dot <= 0) {
                    return false;
                }
                if dot <= 0 {
                    start < r * r
                } else if dot >= length {
                    let x = i64::from(other.position.x) - i64::from(to.x);
                    let y = i64::from(other.position.y) - i64::from(to.y);
                    x * x + y * y < r * r
                } else {
                    let cross = i128::from(x) * i128::from(dy) - i128::from(y) * i128::from(dx);
                    cross * cross < i128::from(r * r) * i128::from(length)
                }
            },
            |tile| {
                // More than 32 centres in one tile cannot form a separated packing
                // even for infantry. Treat it as a compressed spawn region: an
                // actor inside may escape radially; external traffic must route round.
                let cx = i64::from(tile.x) * 1024 + 512;
                let cy = i64::from(tile.y) * 1024 + 512;
                let x = i64::from(from.x) - cx;
                let y = i64::from(from.y) - cy;
                let box_distance = |at: WorldPosition| {
                    let x =
                        i64::from(at.x) - i64::from(at.x.clamp(tile.x * 1024, (tile.x + 1) * 1024));
                    let y =
                        i64::from(at.y) - i64::from(at.y.clamp(tile.y * 1024, (tile.y + 1) * 1024));
                    x * x + y * y
                };
                if from.tile_floor() == tile {
                    x * dx + y * dy < 0
                } else {
                    box_distance(to) <= box_distance(from)
                }
            },
        )
    }

    /// Circle against adjacent static tile footprints. At tile centres the
    /// largest supported radius fits without additional terrain queries.
    pub(crate) fn body_position_clear(&self, kind: UnitKind, at: WorldPosition) -> bool {
        if !self.config.valid_ground_position(at) {
            return false;
        }
        let r = kind.collision_radius();
        let mut bridge = None;
        let here = at.tile_floor();
        let min = WorldPosition::new(at.x - r, at.y - r).tile_floor();
        let max = WorldPosition::new(at.x + r, at.y + r).tile_floor();
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let tile = TileCoord::new(x, y);
                if tile == here {
                    continue;
                }
                let dx = i64::from(at.x) - i64::from(at.x.clamp(x * 1024, (x + 1) * 1024));
                let dy = i64::from(at.y) - i64::from(at.y.clamp(y * 1024, (y + 1) * 1024));
                if dx * dx + dy * dy < i64::from(r) * i64::from(r)
                    && (self.economy.blocked_tiles.contains(&tile)
                        || (!self
                            .unit_tile_passable(kind, tile, &|| false)
                            .unwrap_or(false)
                            && !(*bridge.get_or_insert_with(|| {
                                !kind.is_naval() && self.on_bridge_corridor(at, r)
                            }) && self.bridge_water_clear(tile))))
                {
                    return false;
                }
            }
        }
        true
    }
}

impl GameWorld {
    // Raster bridge links form a one-tile-wide continuous deck. On a diagonal
    // link its sides cross water tile corners: water there is not a solid wall.
    // Foundations/buildings are still checked independently above.
    pub(super) fn bridge_water_clear(&self, tile: TileCoord) -> bool {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return false;
        };
        !overlay.blocks_tile(tile)
            && generator
                .tile_and_node_with_cancel(tile, &|| false)
                .ok()
                .flatten()
                .is_some_and(|(t, node)| {
                    t.water != aoe_map::WaterKind::None
                        && node.is_none_or(|n| !overlay.blocks_node(n))
                })
    }

    pub(super) fn on_bridge_corridor(&self, at: WorldPosition, radius: i32) -> bool {
        let Terrain::Map { overlay, .. } = &self.terrain else {
            return false;
        };
        let here = at.tile_floor();
        let margin = i128::from(512 - radius);
        for y in -1..=1 {
            for x in -1..=1 {
                let from = TileCoord::new(here.x + x, here.y + y);
                let Some(mask) = overlay.bridge_connections(from) else {
                    continue;
                };
                let Ok(start) = WorldPosition::from_tile_center(from) else {
                    continue;
                };
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if mask & (1 << ((dy + 1) * 3 + dx + 1)) == 0 {
                            continue;
                        }
                        let to = TileCoord::new(from.x + dx, from.y + dy);
                        if overlay.bridge_edge(from, to) != Some(true) {
                            continue;
                        }
                        let vx = i128::from(dx * 1024);
                        let vy = i128::from(dy * 1024);
                        let px = i128::from(at.x - start.x);
                        let py = i128::from(at.y - start.y);
                        let len = vx * vx + vy * vy;
                        let dot = px * vx + py * vy;
                        if len > 0
                            && dot >= 0
                            && dot <= len
                            && (px * vy - py * vx).pow(2) <= margin * margin * len
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}
