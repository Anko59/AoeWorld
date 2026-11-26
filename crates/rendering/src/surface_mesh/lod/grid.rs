//! Bounded visible-cell indexing shared by fine and coarse mesh selection.
use super::*;

#[cfg(test)]
pub(super) use tests::VISIBILITY_CHECKS;

/// Fine visibility depends on the camera and source heights, never the mesh LOD.
pub(super) fn visible_tiles(
    terrain: &[SceneTerrain],
    camera: &Camera,
) -> ([[i32; 2]; 2], Vec<bool>) {
    let mut min = [i32::MAX; 2];
    let mut max = [i32::MIN; 2];
    let visible = terrain
        .iter()
        .map(|sample| {
            let tile = tile_key(sample.position);
            let intersects = fine_tile_visible(camera, *sample, tile, camera.viewport);
            if intersects {
                for axis in 0..2 {
                    min[axis] = min[axis].min(tile[axis]);
                    max[axis] = max[axis].max(tile[axis]);
                }
            }
            intersects
        })
        .collect();
    ([min, max], visible)
}

pub(super) fn visible_grid([min, max]: [[i32; 2]; 2], size: i32) -> Option<CellGrid> {
    let (min, max) = (cell_key(min, size), cell_key(max, size));
    let [width, height] =
        [0, 1].map(|axis| (i64::from(max[axis]) - i64::from(min[axis])) / i64::from(size) + 1);
    // Reserve room for at least 2×2 art patches on coarse cells without raising
    // the existing triangle allocation bound. Heights still use one coarse cell.
    let limit = if size == 1 {
        MAX_SURFACE_TRIANGLES
    } else {
        MAX_SURFACE_TILES.min(MAX_SURFACE_TRIANGLES / 8)
    };
    (width.checked_mul(height)? <= limit as i64).then_some(CellGrid {
        min,
        size,
        width: width as usize,
        height: height as usize,
    })
}

pub(super) fn fine_tile_visible(
    camera: &Camera,
    sample: SceneTerrain,
    tile: [i32; 2],
    viewport: [f64; 2],
) -> bool {
    #[cfg(test)]
    VISIBILITY_CHECKS.with(|checks| checks.set(checks.get() + 1));
    let heights = corner_heights(sample.surface.corner_game_height_levels);
    intersects_viewport(&projected_corners(camera, tile, 1, heights), viewport)
}

#[cfg(test)]
mod tests {
    thread_local! {
        pub(in super::super) static VISIBILITY_CHECKS: std::cell::Cell<usize> = const {
            std::cell::Cell::new(0)
        };
    }
}

pub(super) fn intersects_viewport(points: &[SurfacePoint; 4], viewport: [f64; 2]) -> bool {
    let min_x = points
        .iter()
        .map(|point| point.screen.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|point| point.screen.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = points
        .iter()
        .map(|point| point.screen.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = points
        .iter()
        .map(|point| point.screen.y)
        .fold(f64::NEG_INFINITY, f64::max);
    min_x <= viewport[0] && max_x >= 0.0 && min_y <= viewport[1] && max_y >= 0.0
}

pub(super) fn projected_corners(
    camera: &Camera,
    tile: [i32; 2],
    size: i32,
    heights: [f64; 4],
) -> [SurfacePoint; 4] {
    let x = f64::from(tile[0]);
    let y = f64::from(tile[1]);
    let step = f64::from(size);
    [
        project_world(camera, [x, y], heights[0]),
        project_world(camera, [x + step, y], heights[1]),
        project_world(camera, [x + step, y + step], heights[2]),
        project_world(camera, [x, y + step], heights[3]),
    ]
}

pub(super) fn corner_heights(heights: [i16; 4]) -> [f64; 4] {
    [
        f64::from(heights[0]),
        f64::from(heights[1]),
        f64::from(heights[2]),
        f64::from(heights[3]),
    ]
}

pub(super) fn cell_key(tile: [i32; 2], size: i32) -> [i32; 2] {
    [
        tile[0].div_euclid(size) * size,
        tile[1].div_euclid(size) * size,
    ]
}

pub(super) fn grid_index(grid: CellGrid, tile: [i32; 2]) -> Option<usize> {
    let x = (i64::from(tile[0]) - i64::from(grid.min[0])) / i64::from(grid.size);
    let y = (i64::from(tile[1]) - i64::from(grid.min[1])) / i64::from(grid.size);
    (x >= 0 && y >= 0 && x < grid.width as i64 && y < grid.height as i64)
        .then_some(y as usize * grid.width + x as usize)
}

pub(super) fn grid_cell(grid: CellGrid, index: usize) -> [i32; 2] {
    [
        grid.min[0] + (index % grid.width) as i32 * grid.size,
        grid.min[1] + (index / grid.width) as i32 * grid.size,
    ]
}

pub(super) fn cell_center(tile: [i32; 2], size: i32) -> [f64; 2] {
    let offset = f64::from(size) * 0.5;
    [f64::from(tile[0]) + offset, f64::from(tile[1]) + offset]
}

pub(super) fn grid_neighbor(
    cells: &[Option<CellSample>],
    grid: CellGrid,
    tile: [i32; 2],
) -> Option<&SceneTerrain> {
    let index = grid_index(grid, tile)?;
    cells[index].as_ref().map(|cell| &cell.sample)
}
