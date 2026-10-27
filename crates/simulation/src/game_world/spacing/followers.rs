//! Ordered units only: idle crowds cannot consume follower sample windows.
use aoe_core::{EntityId, TileCoord};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub(crate) struct Followers {
    cells: HashMap<u64, Vec<EntityId>>,
    slots: Vec<usize>,
    #[cfg(test)]
    neighbour_reads: std::sync::atomic::AtomicUsize,
}
fn packed(x: i32, y: i32) -> u64 {
    u64::from(x as u32) | (u64::from(y as u32) << 32)
}
fn key(tile: TileCoord) -> u64 {
    packed(tile.x >> 3, tile.y >> 3)
}
#[cfg(not(target_arch = "wasm32"))]
mod snapshot;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use snapshot::Snapshot;

fn sample_cell(
    units: &[EntityId],
    salt: u64,
    except: EntityId,
    test: &mut impl FnMut(EntityId) -> bool,
) -> bool {
    if units.len() <= 32 {
        return units.iter().copied().any(|id| id != except && test(id));
    }
    let start = (salt % units.len() as u64) as usize;
    let first = (units.len() - start).min(32);
    units[start..start + first]
        .iter()
        .chain(&units[..32 - first])
        .copied()
        .any(|id| id != except && test(id))
}

impl Followers {
    fn cell(&self, x: i32, y: i32) -> Option<&[EntityId]> {
        #[cfg(test)]
        self.neighbour_reads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.cells.get(&packed(x, y)).map(Vec::as_slice)
    }

    #[cfg(test)]
    pub(crate) fn reset_neighbour_lookups(&self) {
        self.neighbour_reads
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn neighbour_lookups(&self) -> usize {
        self.neighbour_reads
            .load(std::sync::atomic::Ordering::Relaxed)
    }

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
    /// Same bounded cyclic windows for live ordered application and snapshots.
    pub(crate) fn any_neighbour(
        &self,
        tile: TileCoord,
        salt: u64,
        except: EntityId,
        mut test: impl FnMut(EntityId) -> bool,
    ) -> bool {
        for y in ((tile.y - 1) >> 3)..=((tile.y + 1) >> 3) {
            for x in ((tile.x - 1) >> 3)..=((tile.x + 1) >> 3) {
                if let Some(units) = self.cell(x, y)
                    && sample_cell(units, salt, except, &mut test)
                {
                    return true;
                }
            }
        }
        false
    }
}
