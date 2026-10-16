//! Ground detail shares the unchanged allocation with terrain closing faces.
use super::*;

pub(super) fn subdivisions(cells: &[Option<CellSample>], grid: CellGrid, skirts: usize) -> i32 {
    let mut divisions = appearance::texture_subdivisions(grid.size, cells.len());
    // Fine cells are budgeted by their actual tops and closing edges before
    // rendering. Coarse cells also reserve their possible closing faces.
    if divisions == 1 {
        return divisions;
    }
    while divisions > 1
        && cells
            .len()
            .saturating_mul(2)
            .saturating_mul(divisions as usize)
            .saturating_mul(divisions as usize)
            .saturating_add(skirts)
            > MAX_SURFACE_TRIANGLES
    {
        divisions /= 2;
    }
    divisions
}

#[inline(never)]
pub(super) fn surface_counts(cells: &[Option<CellSample>], grid: CellGrid) -> (usize, usize) {
    let mut tops = 0;
    let mut skirts = 0;
    for (index, cell) in cells.iter().enumerate() {
        let Some(cell) = cell else { continue };
        tops += 2;
        let tile = grid_cell(grid, index);
        for offset in [[grid.size, 0], [0, grid.size]] {
            let Some(neighbor) =
                grid_neighbor(cells, grid, [tile[0] + offset[0], tile[1] + offset[1]])
            else {
                continue;
            };
            let sample = cell.sample;
            let eligible = if grid.size == 1 {
                (sample.surface.water != 0
                    || neighbor.surface.water != 0
                    || sample.surface.is_cliff()
                    || neighbor.surface.is_cliff())
                    && {
                        let a = sample.surface.corner_game_height_levels;
                        let b = neighbor.surface.corner_game_height_levels;
                        if offset[0] != 0 {
                            a[1] != b[0] || a[2] != b[3]
                        } else {
                            a[2] != b[1] || a[3] != b[0]
                        }
                    }
            } else {
                sample.surface.kind == SceneTerrainSurface::CLIFF_LEDGE
                    || neighbor.surface.kind == SceneTerrainSurface::CLIFF_LEDGE
            };
            if eligible {
                // Each eligible edge emits at most two triangles, including
                // crossed sheets. Equal heights can use less than this reserve.
                skirts += 2;
            }
        }
    }
    (tops, skirts)
}
