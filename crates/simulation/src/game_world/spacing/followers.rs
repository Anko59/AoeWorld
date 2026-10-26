//! Ordered units only: idle crowds cannot consume follower sample windows.
use aoe_core::{EntityId, TileCoord};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub(crate) struct Followers {
    cells: HashMap<u64, Vec<EntityId>>,
    slots: Vec<usize>,
}
fn packed(x: i32, y: i32) -> u64 {
    u64::from(x as u32) | (u64::from(y as u32) << 32)
}
fn key(tile: TileCoord) -> u64 {
    packed(tile.x >> 3, tile.y >> 3)
}
impl Followers {
    pub(crate) fn contains(&self, id: EntityId) -> bool {
        self.slots
            .get(id.0 as usize)
            .is_some_and(|&slot| slot != usize::MAX)
    }
    pub(crate) fn sampling_slot(&self, id: EntityId) -> u64 {
        self.slots
            .get(id.0 as usize)
            .filter(|&&slot| slot != usize::MAX)
            .map_or(u64::MAX, |&slot| slot as u64)
    }
    pub(crate) fn insert(&mut self, id: EntityId, tile: TileCoord) {
        if self.contains(id) {
            return;
        }
        if id.0 as usize >= self.slots.len() {
            self.slots.resize(id.0 as usize + 1, usize::MAX);
        }
        let units = self.cells.entry(key(tile)).or_default();
        self.slots[id.0 as usize] = units.len();
        units.push(id);
    }
    pub(crate) fn remove(&mut self, id: EntityId, tile: TileCoord) {
        let Some(&slot) = self
            .slots
            .get(id.0 as usize)
            .filter(|&&slot| slot != usize::MAX)
        else {
            return;
        };
        let key = key(tile);
        let Some(units) = self.cells.get_mut(&key) else {
            return;
        };
        if units.get(slot) != Some(&id) {
            return;
        }
        units.swap_remove(slot);
        self.slots[id.0 as usize] = usize::MAX;
        if let Some(displaced) = units.get(slot) {
            self.slots[displaced.0 as usize] = slot;
        }
        if units.is_empty() {
            self.cells.remove(&key);
        }
    }
    pub(crate) fn relocate(&mut self, id: EntityId, from: TileCoord, to: TileCoord) {
        if from != to && key(from) != key(to) {
            self.remove(id, from);
            self.insert(id, to);
        }
    }
    /// Four coarse cells at most; 32 cyclic inspections per cell, including
    /// the querying actor. Small cells need neither modulo nor iterator setup.
    pub(crate) fn any_neighbour(
        &self,
        tile: TileCoord,
        salt: u64,
        except: EntityId,
        mut test: impl FnMut(EntityId) -> bool,
    ) -> bool {
        for y in ((tile.y - 1) >> 3)..=((tile.y + 1) >> 3) {
            for x in ((tile.x - 1) >> 3)..=((tile.x + 1) >> 3) {
                let Some(units) = self.cells.get(&packed(x, y)) else {
                    continue;
                };
                if units.len() <= 32 {
                    for &id in units {
                        if id != except && test(id) {
                            return true;
                        }
                    }
                    continue;
                }
                let start = (salt % units.len() as u64) as usize;
                let first = (units.len() - start).min(32);
                for &id in &units[start..start + first] {
                    if id != except && test(id) {
                        return true;
                    }
                }
                for &id in &units[..32 - first] {
                    if id != except && test(id) {
                        return true;
                    }
                }
            }
        }
        false
    }
}
