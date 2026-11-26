//! Which of its eight steps a leg search may take from each tile, kept
//! across ticks. A search expanding a tile read up to a dozen ground and
//! edge answers per step (gate, bridge, walk and chunk-edge lookups, the
//! ground behind a lock); with every route search polled every tick (#978)
//! those reads were half of all route work. One atomic read replaces them.
//! Workers fill tiles concurrently with the same answer; only a collision
//! change (with exclusive access) forgets them.
use aoe_core::{TileCoord, TileRect};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU16, Ordering};

const SIDE: i32 = 64;
const KNOWN: u16 = 1 << 15;
/// The tile's steps need the caller's slow test (a gate is near).
const SLOW: u16 = 1 << 14;

/// Set in a known mask when the tile itself is walkable.
pub const STEP_MASK_WALKABLE: u16 = 1 << 8;

/// The steps a tile's search may take, as `route_neighbor_clear` answers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepMask {
    Unknown,
    Slow,
    /// Bit `i` is the step by `neighbor_offset(i)` (northwest first, in
    /// row order); `STEP_MASK_WALKABLE` is the tile's own ground.
    Known(u16),
}

#[derive(Debug, Default)]
pub struct RouteStepMasks {
    grid: OnceLock<Grid>,
}

#[derive(Debug)]
struct Grid {
    columns: i32,
    rows: i32,
    chunks: Box<[OnceLock<Box<[AtomicU16]>>]>,
}

impl Grid {
    fn index(&self, tile: TileCoord) -> Option<(usize, usize)> {
        let (cx, cy) = (tile.x.div_euclid(SIDE), tile.y.div_euclid(SIDE));
        if tile.x < 0 || tile.y < 0 || cx >= self.columns || cy >= self.rows {
            return None;
        }
        let inner = (tile.y.rem_euclid(SIDE) * SIDE + tile.x.rem_euclid(SIDE)) as usize;
        Some(((cy * self.columns + cx) as usize, inner))
    }
}

/// The index of a step offset in a `StepMask::Known` mask.
pub fn step_index((dx, dy): (i32, i32)) -> Option<u32> {
    (dx.abs() <= 1 && dy.abs() <= 1 && (dx, dy) != (0, 0)).then(|| {
        let index = (dy + 1) * 3 + dx + 1;
        (if index > 4 { index - 1 } else { index }) as u32
    })
}

impl RouteStepMasks {
    pub fn mask(&self, tile: TileCoord) -> StepMask {
        let Some(grid) = self.grid.get() else {
            return StepMask::Unknown;
        };
        let Some((chunk, inner)) = grid.index(tile) else {
            return StepMask::Unknown;
        };
        let Some(values) = grid.chunks[chunk].get() else {
            return StepMask::Unknown;
        };
        let value = values[inner].load(Ordering::Relaxed);
        match value {
            _ if value & KNOWN == 0 => StepMask::Unknown,
            _ if value & SLOW != 0 => StepMask::Slow,
            _ => StepMask::Known(value & 0x1ff),
        }
    }

    /// Records `tile`'s steps (`None`: slow) on a map of the given size in
    /// tiles. Tiles off the map are not recorded.
    pub fn store(&self, width: i32, height: i32, tile: TileCoord, mask: Option<u16>) {
        let grid = self.grid.get_or_init(|| {
            let columns = (width.max(0) + SIDE - 1) / SIDE;
            let rows = (height.max(0) + SIDE - 1) / SIDE;
            Grid {
                columns,
                rows,
                chunks: (0..columns * rows).map(|_| OnceLock::new()).collect(),
            }
        });
        let Some((chunk, inner)) = grid.index(tile) else {
            return;
        };
        let values = grid.chunks[chunk]
            .get_or_init(|| (0..SIDE * SIDE).map(|_| AtomicU16::new(0)).collect());
        let value = KNOWN | mask.map_or(SLOW, |mask| mask & 0x1ff);
        values[inner].store(value, Ordering::Relaxed);
    }

    /// Forgets every tile whose steps can read a tile of `area`: its own and
    /// those one tile around it (a step reads its target and two corners).
    pub fn invalidate(&mut self, area: TileRect) {
        if area.min.x >= area.max.x || area.min.y >= area.max.y {
            return;
        }
        let Some(grid) = self.grid.get_mut() else {
            return;
        };
        let (min_x, min_y) = (
            area.min.x.saturating_sub(1).max(0),
            area.min.y.saturating_sub(1).max(0),
        );
        let max_x = area.max.x.min(grid.columns * SIDE - 1);
        let max_y = area.max.y.min(grid.rows * SIDE - 1);
        if i64::from(max_x - min_x + 1) * i64::from(max_y - min_y + 1)
            > grid.chunks.len() as i64 * i64::from(SIDE * SIDE)
        {
            grid.chunks.iter_mut().for_each(|chunk| drop(chunk.take()));
            return;
        }
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let Some((chunk, inner)) = grid.index(TileCoord::new(x, y)) else {
                    continue;
                };
                if let Some(values) = grid.chunks[chunk].get_mut() {
                    *values[inner].get_mut() = 0;
                }
            }
        }
    }

    pub fn clear(&mut self) {
        if let Some(grid) = self.grid.get_mut() {
            for chunk in grid.chunks.iter_mut() {
                chunk.take();
            }
        }
    }
}

#[path = "step_masks/tests.rs"]
#[cfg(test)]
mod tests;
