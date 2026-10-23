//! Conservative idle-separation broad phase, maintained when units change tiles.
//! Hash maps are used only for point lookups; candidate traversal is ordered.
use aoe_core::{EntityId, TileCoord};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Default)]
pub(crate) struct IdleSpacing {
    cells: HashMap<u64, Cell>,
    candidates: [BTreeSet<EntityId>; 4],
    #[cfg(test)]
    pub(crate) inspected_units: usize,
    #[cfg(test)]
    pub(crate) adjacent_visits: usize,
}

#[derive(Debug, Default)]
struct Cell {
    units: BTreeSet<EntityId>,
    /// Occupied neighbouring tiles, excluding this tile (at most eight).
    adjacent: u8,
}

fn key(tile: TileCoord) -> u64 {
    u64::from(tile.x as u32) | (u64::from(tile.y as u32) << 32)
}

fn adjacent(tile: TileCoord) -> impl Iterator<Item = TileCoord> {
    (-1..=1)
        .flat_map(move |y| (-1..=1).map(move |x| TileCoord::new(tile.x + x, tile.y + y)))
        .filter(move |other| *other != tile)
}

impl IdleSpacing {
    pub(crate) fn candidates(&self, phase: usize) -> impl Iterator<Item = EntityId> + '_ {
        self.candidates[phase].iter().copied()
    }

    pub(crate) fn insert(&mut self, id: EntityId, tile: TileCoord) {
        let cell = self.cells.entry(key(tile)).or_default();
        if !cell.units.insert(id) {
            return;
        }
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
        let mut occupied = 0;
        for other in adjacent(tile) {
            #[cfg(test)]
            {
                self.adjacent_visits += 1;
            }
            let Some(cell) = self.cells.get_mut(&key(other)) else {
                continue;
            };
            occupied += 1;
            if cell.adjacent == 0 && cell.units.len() == 1 {
                let other_id = *cell.units.first().expect("one unit");
                self.candidates[other_id.0 as usize % 4].insert(other_id);
            }
            cell.adjacent += 1;
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
        let Some(cell) = self.cells.get_mut(&key(tile)) else {
            return;
        };
        if !cell.units.remove(&id) {
            return;
        }
        self.candidates[id.0 as usize % 4].remove(&id);
        match cell.units.len() {
            0 => {
                self.cells.remove(&key(tile));
                for other in adjacent(tile) {
                    #[cfg(test)]
                    {
                        self.adjacent_visits += 1;
                    }
                    let Some(cell) = self.cells.get_mut(&key(other)) else {
                        continue;
                    };
                    cell.adjacent -= 1;
                    if cell.adjacent == 0 && cell.units.len() == 1 {
                        let other_id = *cell.units.first().expect("one unit");
                        self.candidates[other_id.0 as usize % 4].remove(&other_id);
                    }
                }
            }
            1 if cell.adjacent == 0 => {
                let remaining = *cell.units.first().expect("one unit");
                self.candidates[remaining.0 as usize % 4].remove(&remaining);
            }
            _ => {}
        }
    }

    pub(crate) fn relocate(&mut self, id: EntityId, from: TileCoord, to: TileCoord) {
        if from != to {
            self.remove(id, from);
            self.insert(id, to);
        }
    }
}
