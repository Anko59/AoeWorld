//! Bounded visible-cell indexing shared by fine and coarse mesh selection.
use super::*;

pub(super) fn grid_area(min: [i32; 2], max: [i32; 2], size: i32, limit: usize) -> Option<usize> {
    if size <= 0 || min[0] > max[0] || min[1] > max[1] {
        return None;
    }
    let size = i64::from(size);
    let min_x = i64::from(min[0]).div_euclid(size);
    let max_x = i64::from(max[0]).div_euclid(size);
    let min_y = i64::from(min[1]).div_euclid(size);
    let max_y = i64::from(max[1]).div_euclid(size);
    let width = max_x - min_x + 1;
    let height = max_y - min_y + 1;
    if width <= 0 || height <= 0 {
        return None;
    }
    let area = width.checked_mul(height)?;
    (area <= limit as i64).then_some(area as usize)
}

pub(super) fn visible_grid(
    terrain: &[SceneTerrain],
    camera: &Camera,
    viewport: [f64; 2],
    size: i32,
    limit: usize,
) -> Result<Option<CellGrid>, ()> {
    let mut min = [i32::MAX; 2];
    let mut max = [i32::MIN; 2];
    for sample in terrain {
        let tile = tile_key(sample.position);
        if !fine_tile_visible(camera, *sample, tile, viewport) {
            continue;
        }
        let cell = cell_key(tile, size);
        for axis in 0..2 {
            min[axis] = min[axis].min(cell[axis]);
            max[axis] = max[axis].max(cell[axis]);
        }
        if grid_area(min, max, size, limit).is_none() {
            return Err(());
        }
    }
    if min[0] == i32::MAX {
        return Ok(None);
    }
    let area = grid_area(min, max, size, limit).ok_or(())?;
    // Reserve room for at least 2×2 art patches on coarse cells without raising
    // the existing triangle allocation bound. Heights still use one coarse cell.
    if size > 1 && area > MAX_SURFACE_TRIANGLES / 8 {
        return Err(());
    }
    let width = ((i64::from(max[0]) - i64::from(min[0])) / i64::from(size) + 1) as usize;
    let height = ((i64::from(max[1]) - i64::from(min[1])) / i64::from(size) + 1) as usize;
    debug_assert_eq!(width * height, area);
    Ok(Some(CellGrid {
        min,
        size,
        width,
        height,
    }))
}

pub(super) fn fine_tile_visible(
    camera: &Camera,
    sample: SceneTerrain,
    tile: [i32; 2],
    viewport: [f64; 2],
) -> bool {
    let heights = corner_heights(sample.surface.corner_game_height_levels);
    intersects_viewport(&projected_corners(camera, tile, 1, heights), viewport)
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
