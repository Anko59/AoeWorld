//! Fine bodies and conservative coarse population bounds share one index.
use crate::UnitKind;
use aoe_core::{EntityId, TileCoord, WorldPosition};
use std::{
    collections::HashMap,
    hash::{BuildHasherDefault, Hasher},
};

const CELL_LIMIT: usize = 32;
type Cells = HashMap<u64, Vec<EntityId>, BuildHasherDefault<CellHasher>>;

// Coordinates are internal integer keys. Mix the packed pair once instead of
// running the general-purpose keyed byte hasher on every neighbour probe.
#[derive(Debug, Default)]
pub(crate) struct CellHasher(u64);
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

pub(crate) fn key(tile: TileCoord) -> u64 {
    u64::from(tile.x as u32) | (u64::from(tile.y as u32) << 32)
}

#[derive(Debug, Default)]
pub(crate) struct CollisionIndex {
    cells: [Cells; 2],
    slots: Vec<usize>,
    indices: Vec<usize>,
    recycled: Vec<Vec<EntityId>>,
    overfull_cells: usize,
    maximum_radii: [i32; 2],
    land_counts: HashMap<u64, usize, BuildHasherDefault<CellHasher>>,
}

impl CollisionIndex {
    /// Kinds are fixed at spawn. Retain the largest observed radius after
    /// removal: an upper bound avoids a population scan on every group tick.
    pub(crate) fn observe_kind(&mut self, kind: UnitKind) {
        let radius = &mut self.maximum_radii[usize::from(kind.is_naval())];
        *radius = (*radius).max(kind.collision_radius());
    }

    pub(crate) fn maximum_radii(&self) -> [i32; 2] {
        self.maximum_radii
    }

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
        self.overfull_cells += usize::from(cell.len() == CELL_LIMIT + 1);
        if !naval {
            *self.land_counts.entry(coarse(tile)).or_default() += 1;
        }
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
        if !naval {
            let at = coarse(tile);
            if let Some(count) = self.land_counts.get_mut(&at) {
                *count -= 1;
                if *count == 0 {
                    self.land_counts.remove(&at);
                }
            }
        }
        self.overfull_cells -= usize::from(cell.len() == CELL_LIMIT + 1);
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

    pub(crate) fn has_overfull_cells(&self) -> bool {
        self.overfull_cells != 0
    }

    /// Coarse counts conservatively reject sparse contact windows in at most
    /// nine cell reads. Dead/enemy bodies overestimate demand, never hide it.
    pub(crate) fn contact_may_be_crowded(
        &self,
        at: WorldPosition,
        capacity: usize,
        work: &crate::game_world::route_scheduler::GroupWork,
    ) -> bool {
        let min =
            WorldPosition::new(at.x.saturating_sub(1536), at.y.saturating_sub(1536)).tile_floor();
        let max =
            WorldPosition::new(at.x.saturating_add(1536), at.y.saturating_add(1536)).tile_floor();
        let needed = capacity.saturating_sub(1).min(CELL_LIMIT + 1);
        let mut count = 0;
        for y in min.y.div_euclid(2)..=max.y.div_euclid(2) {
            for x in min.x.div_euclid(2)..=max.x.div_euclid(2) {
                work.collision(1);
                count += self
                    .land_counts
                    .get(&key(TileCoord::new(x, y)))
                    .copied()
                    .unwrap_or(0);
                if count >= needed {
                    return true;
                }
            }
        }
        false
    }

    /// At most sixteen cells, with 32 body tests per cell. Hash iteration never
    /// determines a decision: cells and each cell's cyclic window are canonical.
    pub(crate) fn blocked(
        &self,
        (at, reach): (WorldPosition, i32),
        naval: bool,
        salt: u64,
        work: &crate::game_world::route_scheduler::GroupWork,
        mut test: impl FnMut(EntityId) -> bool,
        mut overflow: impl FnMut(TileCoord) -> bool,
    ) -> bool {
        let min =
            WorldPosition::new(at.x.saturating_sub(reach), at.y.saturating_sub(reach)).tile_floor();
        let max =
            WorldPosition::new(at.x.saturating_add(reach), at.y.saturating_add(reach)).tile_floor();
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                work.collision(1);
                let tile = TileCoord::new(x, y);
                let Some(cell) = self.cells[usize::from(naval)].get(&key(tile)) else {
                    continue;
                };
                if cell.len() > CELL_LIMIT && overflow(tile) {
                    return true;
                }
                let len = cell.len();
                let mut at = if len == 1 {
                    0
                } else {
                    (salt % len as u64) as usize
                };
                for _ in 0..len.min(CELL_LIMIT) {
                    work.collision(1);
                    if test(cell[at]) {
                        return true;
                    }
                    at += 1;
                    if at == len {
                        at = 0;
                    }
                }
            }
        }
        false
    }
}

fn coarse(tile: TileCoord) -> u64 {
    key(TileCoord::new(tile.x.div_euclid(2), tile.y.div_euclid(2)))
}

#[cfg(test)]
mod tests;
