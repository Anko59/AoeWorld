//! Idle occupancy and deterministic bounded separation work.
use super::cells::{Cell, OFFSETS, adjacent, key};
use aoe_core::{EntityId, TileCoord};
use std::collections::{BTreeSet, HashMap};
use std::ops::Bound::{Excluded, Unbounded};
const IDLE_VISITS_PER_TICK: usize = 32;
#[derive(Debug, Default)]
pub(crate) struct IdleSpacing {
    cells: HashMap<u64, Cell>,
    slots: Vec<usize>,
    candidates: [BTreeSet<EntityId>; 4],
    cursor: [Option<EntityId>; 4],
    #[cfg(test)]
    pub(crate) inspected_units: usize,
    #[cfg(test)]
    pub(crate) adjacent_visits: usize,
}
impl IdleSpacing {
    pub(crate) fn take_candidates(&mut self, phase: usize) -> Vec<EntityId> {
        let candidates = &self.candidates[phase];
        if candidates.len() <= IDLE_VISITS_PER_TICK {
            self.cursor[phase] = None;
            return candidates.iter().copied().collect();
        }
        let start = self.cursor[phase].map_or(Unbounded, Excluded);
        let batch: Vec<_> = candidates
            .range((start, Unbounded))
            .chain(candidates.iter())
            .take(IDLE_VISITS_PER_TICK)
            .copied()
            .collect();
        self.cursor[phase] = batch.last().copied();
        batch
    }
    pub(crate) fn update_hash(&self, hash: &mut blake3::Hasher) {
        for cursor in self.cursor {
            hash.update(&cursor.map_or(u64::MAX, |id| u64::from(id.0)).to_le_bytes());
        }
    }
    #[cfg(test)]
    pub(crate) fn candidates(&self, phase: usize) -> impl Iterator<Item = EntityId> + '_ {
        self.candidates[phase].iter().copied()
    }
    pub(crate) fn insert(&mut self, id: EntityId, tile: TileCoord) {
        let at = id.0 as usize;
        if self.slots.get(at).is_some_and(|&slot| slot != usize::MAX) {
            return;
        }
        if at >= self.slots.len() {
            self.slots.resize(at + 1, usize::MAX);
        }
        let cell = self.cells.entry(key(tile)).or_default();
        self.slots[at] = cell.units.len();
        cell.units.push(id);
        match cell.units.len() {
            1 => {}
            2 if cell.adjacent == 0 => {
                for &id in &cell.units {
                    self.candidates[id.0 as usize % 4].insert(id);
                }
                return;
            }
            _ => {
                self.candidates[id.0 as usize % 4].insert(id);
                return;
            }
        }
        let mut occupied = 0_u8;
        for (direction, other) in adjacent(tile).enumerate() {
            #[cfg(test)]
            {
                self.adjacent_visits += 1;
            }
            let Some(cell) = self.cells.get_mut(&key(other)) else {
                continue;
            };
            occupied |= 1 << direction;
            if cell.adjacent == 0 && cell.units.len() == 1 {
                let other = cell.units[0];
                self.candidates[other.0 as usize % 4].insert(other);
            }
            cell.adjacent |= 1 << (7 - direction);
        }
        self.cells
            .get_mut(&key(tile))
            .expect("inserted tile")
            .adjacent = occupied;
        if occupied > 0 {
            self.candidates[id.0 as usize % 4].insert(id);
        }
    }
    pub(crate) fn remove(&mut self, id: EntityId, tile: TileCoord) {
        let Some(&slot) = self
            .slots
            .get(id.0 as usize)
            .filter(|&&slot| slot != usize::MAX)
        else {
            return;
        };
        let Some(cell) = self.cells.get_mut(&key(tile)) else {
            return;
        };
        if cell.units.get(slot) != Some(&id) {
            return;
        }
        cell.units.swap_remove(slot);
        self.slots[id.0 as usize] = usize::MAX;
        if let Some(displaced) = cell.units.get(slot) {
            self.slots[displaced.0 as usize] = slot;
        }
        self.candidates[id.0 as usize % 4].remove(&id);
        match cell.units.len() {
            0 => {
                let mut occupied = cell.adjacent;
                self.cells.remove(&key(tile));
                while occupied != 0 {
                    let direction = occupied.trailing_zeros() as usize;
                    occupied &= occupied - 1;
                    let (x, y) = OFFSETS[direction];
                    #[cfg(test)]
                    {
                        self.adjacent_visits += 1;
                    }
                    let Some(cell) = self
                        .cells
                        .get_mut(&key(TileCoord::new(tile.x + x, tile.y + y)))
                    else {
                        continue;
                    };
                    cell.adjacent &= !(1 << (7 - direction));
                    if cell.adjacent == 0 && cell.units.len() == 1 {
                        let other = cell.units[0];
                        self.candidates[other.0 as usize % 4].remove(&other);
                    }
                }
            }
            1 if cell.adjacent == 0 => {
                let other = cell.units[0];
                self.candidates[other.0 as usize % 4].remove(&other);
            }
            _ => {}
        }
    }
    pub(crate) fn relocate(&mut self, id: EntityId, from: TileCoord, to: TileCoord) {
        if from == to {
            return;
        }
        // Reuse an isolated unit's empty 3x3 window. A one-tile step exposes
        // only three (axis) or five (diagonal) new tiles, without reallocating.
        if from.x.abs_diff(to.x) <= 1
            && from.y.abs_diff(to.y) <= 1
            && self
                .cells
                .get(&key(from))
                .is_some_and(|cell| cell.adjacent == 0 && cell.units.as_slice() == [id])
        {
            let mut cell = self.cells.remove(&key(from)).expect("isolated cell");
            for (direction, other) in adjacent(to).enumerate() {
                if other.x.abs_diff(from.x) <= 1 && other.y.abs_diff(from.y) <= 1 {
                    continue;
                }
                #[cfg(test)]
                {
                    self.adjacent_visits += 1;
                }
                let Some(neighbour) = self.cells.get_mut(&key(other)) else {
                    continue;
                };
                cell.adjacent |= 1 << direction;
                if neighbour.adjacent == 0 && neighbour.units.len() == 1 {
                    let other = neighbour.units[0];
                    self.candidates[other.0 as usize % 4].insert(other);
                }
                neighbour.adjacent |= 1 << (7 - direction);
            }
            if cell.adjacent != 0 {
                self.candidates[id.0 as usize % 4].insert(id);
            }
            self.cells.insert(key(to), cell);
            return;
        }
        self.remove(id, from);
        self.insert(id, to);
    }
}
