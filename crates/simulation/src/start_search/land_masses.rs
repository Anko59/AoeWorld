//! Coarse land masses, so a start can be required to lie on the main
//! continent. There are no transport ships: a village on an island could never
//! reach any other player by land.
//!
//! The map is cut into chunk-sized cells. A cell counts as land unless coarse
//! hydrology proves every tile of it is ocean or lake (microseconds, no chunk
//! composition), and 4-connected land cells form one mass. The test is
//! deliberately generous, so a narrow isthmus is never cut, while a channel of
//! a chunk or more separates masses. One flood fill per match gives an id per
//! cell; a candidate is then checked by a table lookup, not a route search.
use super::Terrain;
use aoe_core::{TileCoord, WorldConfig};
use aoe_map::CHUNK_TILES;

#[derive(Clone, Debug)]
pub struct LandMasses {
    cells: i32,
    /// Mass id per cell, row-major; 0 is water.
    mass: Vec<u32>,
    main: u32,
    count: usize,
}

impl LandMasses {
    /// Labels the masses of a `cells` x `cells` grid. `None` when nothing is land.
    pub fn from_cells(cells: i32, mut land: impl FnMut(i32, i32) -> bool) -> Option<Self> {
        let side = cells.max(0) as usize;
        let mut mass = vec![0_u32; side * side];
        let mut sizes = vec![0_usize];
        let mut stack = Vec::new();
        for start in 0..side * side {
            let (x, y) = ((start % side) as i32, (start / side) as i32);
            if mass[start] != 0 || !land(x, y) {
                continue;
            }
            let id = sizes.len() as u32;
            sizes.push(0);
            mass[start] = id;
            stack.push((x, y));
            while let Some((x, y)) = stack.pop() {
                sizes[id as usize] += 1;
                for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= cells || ny >= cells {
                        continue;
                    }
                    let at = ny as usize * side + nx as usize;
                    if mass[at] == 0 && land(nx, ny) {
                        mass[at] = id;
                        stack.push((nx, ny));
                    }
                }
            }
        }
        // Largest mass; the lowest id breaks a tie, so the result is deterministic.
        let (main, _) = sizes
            .iter()
            .enumerate()
            .skip(1)
            .max_by_key(|(id, size)| (**size, std::cmp::Reverse(*id)))?;
        Some(Self {
            cells,
            mass,
            main: main as u32,
            count: sizes.len() - 1,
        })
    }

    /// Does `tile` lie on the main continent?
    pub fn is_main(&self, tile: TileCoord, cell_tiles: i32) -> bool {
        if tile.x < 0 || tile.y < 0 {
            return false;
        }
        let (x, y) = (tile.x / cell_tiles, tile.y / cell_tiles);
        x < self.cells && y < self.cells && self.mass[(y * self.cells + x) as usize] == self.main
    }

    /// How many separate land masses the map holds.
    pub fn count(&self) -> usize {
        self.count
    }
}

impl Terrain {
    /// The land masses of a source-backed map, `None` for flat ground, maps
    /// too small to cut, or maps without land.
    pub fn land_masses(&self, config: WorldConfig) -> Option<LandMasses> {
        let Self::Map { .. } = self else { return None };
        let side = config.width_tiles;
        if side <= 2 * CHUNK_TILES {
            return None;
        }
        let cells = (side + CHUNK_TILES - 1) / CHUNK_TILES;
        let half = CHUNK_TILES / 2;
        LandMasses::from_cells(cells, |x, y| {
            // The cell's middle, kept inside the margin the water test needs.
            let at = |cell: i32| (cell * CHUNK_TILES + half - 1).clamp(half, side - half - 1);
            !self.open_water_certain(TileCoord::new(at(x), at(y)), half)
        })
    }

    /// Does `tile` lie on the main continent of `masses` (always, without masses)?
    pub fn on_main_land(masses: Option<&LandMasses>, tile: TileCoord) -> bool {
        masses.is_none_or(|masses| masses.is_main(tile, CHUNK_TILES))
    }
}
