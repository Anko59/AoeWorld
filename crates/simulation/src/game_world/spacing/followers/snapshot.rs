//! A bounded, retained cell index for immutable walking proposals.
use super::{EntityId, Followers, TileCoord, sample_cell};

// At most 16 MiB for the cell index: a 16,384-square map. Larger maps
// keep the live lookup path rather than allocating a world-sized dense grid.
const MAX_CELLS: usize = 1 << 22;

#[derive(Debug, Default)]
pub(crate) struct Snapshot {
    columns: usize,
    rows: usize,
    cells: Vec<u32>,
    touched: Vec<usize>,
    ranges: Vec<(usize, usize)>,
    pool: Vec<EntityId>,
}

impl Snapshot {
    pub(crate) fn refresh(&mut self, source: &Followers, width: i32, height: i32) -> bool {
        if width <= 0 || height <= 0 {
            return false;
        }
        let columns = (width as usize).div_ceil(8);
        let rows = (height as usize).div_ceil(8);
        let Some(count) = columns.checked_mul(rows).filter(|n| *n <= MAX_CELLS) else {
            return false;
        };
        // Clear only cells populated last tick, never the entire map grid.
        for at in self.touched.drain(..) {
            self.cells[at] = 0;
        }
        self.cells.resize(count, 0);
        self.columns = columns;
        self.rows = rows;
        self.ranges.clear();
        self.ranges.push((0, 0));
        self.pool.clear();
        for (&key, units) in &source.cells {
            let x = key as u32 as i32;
            let y = (key >> 32) as u32 as i32;
            if x < 0 || y < 0 || x as usize >= columns || y as usize >= rows {
                return false;
            }
            let at = y as usize * columns + x as usize;
            self.cells[at] = self.ranges.len() as u32;
            self.touched.push(at);
            let start = self.pool.len();
            self.pool.extend_from_slice(units);
            self.ranges.push((start, self.pool.len()));
        }
        true
    }

    pub(crate) fn any_neighbour(
        &self,
        tile: TileCoord,
        salt: u64,
        except: EntityId,
        mut test: impl FnMut(EntityId) -> bool,
    ) -> bool {
        for y in ((tile.y - 1) >> 3)..=((tile.y + 1) >> 3) {
            for x in ((tile.x - 1) >> 3)..=((tile.x + 1) >> 3) {
                if x < 0 || y < 0 || x as usize >= self.columns || y as usize >= self.rows {
                    continue;
                }
                let cell = self.cells[y as usize * self.columns + x as usize];
                let (start, end) = self.ranges[cell as usize];
                if sample_cell(&self.pool[start..end], salt, except, &mut test) {
                    return true;
                }
            }
        }
        false
    }

    #[cfg(test)]
    pub(crate) fn cell_count(&self) -> usize {
        self.ranges.len().saturating_sub(1)
    }
}

#[cfg(test)]
mod tests;
