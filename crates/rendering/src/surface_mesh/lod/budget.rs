//! Ground detail shares the unchanged allocation with terrain closing faces.
use super::*;

pub(super) fn subdivisions(cells: &[Option<CellSample>], grid: CellGrid) -> i32 {
    let mut divisions = appearance::texture_subdivisions(grid.size, cells.len());
    // Fine cells already reserve six triangles each: two top triangles and
    // two for each closing edge. Coarse cells previously reserved only tops.
    if divisions == 1 {
        return divisions;
    }
    let skirts = skirt_reserve(cells, grid);
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

pub(super) fn skirt_reserve(cells: &[Option<CellSample>], grid: CellGrid) -> usize {
    let mut skirts = 0;
    for (index, cell) in cells.iter().enumerate() {
        let Some(cell) = cell else { continue };
        let tile = grid_cell(grid, index);
        for offset in [[grid.size, 0], [0, grid.size]] {
            let Some(neighbor) =
                grid_neighbor(cells, grid, [tile[0] + offset[0], tile[1] + offset[1]])
            else {
                continue;
            };
            if cell.sample.surface.kind == SceneTerrainSurface::CLIFF_LEDGE
                || neighbor.surface.kind == SceneTerrainSurface::CLIFF_LEDGE
            {
                // Each eligible edge emits at most two triangles, including
                // crossed sheets. Equal heights can use less than this reserve.
                skirts += 2;
            }
        }
    }
    skirts
}
