use super::*;
use aoe_core::Camera;

#[cfg(test)]
#[path = "lod_tests.rs"]
mod tests;

#[derive(Clone, Copy)]
struct CellSample {
    source_tile: [i32; 2],
    sample: SceneTerrain,
    distance: f64,
}

#[derive(Clone, Copy)]
struct CellGrid {
    min: [i32; 2],
    size: i32,
    width: usize,
    height: usize,
}

struct CoarseHeights {
    vertices: Vec<(f64, u32)>,
    fallback: f64,
}

pub fn projected_surface_triangles(
    terrain: &[SceneTerrain],
    scene_camera: SceneCamera,
) -> Vec<ProjectedSurfaceTriangle> {
    if terrain.is_empty() {
        return Vec::new();
    }
    let camera = Camera {
        center: scene_camera.center,
        zoom: scene_camera.zoom,
        viewport: scene_camera.viewport,
        focus_elevation_meters: scene_camera.focus_elevation_meters,
    };
    let mut size = 1;
    let grid = loop {
        match visible_grid(terrain, &camera, scene_camera.viewport, size) {
            Ok(Some(grid)) => break grid,
            Ok(None) => return Vec::new(),
            Err(()) if size < i32::MAX / 2 => size *= 2,
            Err(()) => return Vec::new(),
        }
    };
    let mut cells = vec![None; grid.width * grid.height];
    for sample in terrain {
        let tile = tile_key(sample.position);
        if !fine_tile_visible(&camera, *sample, tile, scene_camera.viewport) {
            continue;
        }
        let Some(index) = grid_index(grid, cell_key(tile, size)) else {
            continue;
        };
        let center = cell_center(cell_key(tile, size), size);
        let sample_center = [f64::from(tile[0]) + 0.5, f64::from(tile[1]) + 0.5];
        let dx = sample_center[0] - center[0];
        let dy = sample_center[1] - center[1];
        let candidate = CellSample {
            source_tile: tile,
            sample: *sample,
            distance: dx.mul_add(dx, dy * dy),
        };
        let slot = &mut cells[index];
        if slot.is_none_or(|best: CellSample| {
            candidate.distance < best.distance
                || (candidate.distance == best.distance && candidate.source_tile < best.source_tile)
        }) {
            *slot = Some(candidate);
        }
    }

    let coarse_heights = (size > 1).then(|| shared_coarse_heights(terrain, &cells, grid));
    let mut result = Vec::with_capacity(MAX_SURFACE_TRIANGLES);
    for (slot_index, candidate) in cells.iter().enumerate() {
        let Some(candidate) = candidate else {
            continue;
        };
        let tile = grid_cell(grid, slot_index);
        let sample = candidate.sample;
        let heights = coarse_heights.as_ref().map_or(
            corner_heights(sample.surface.corner_game_height_levels),
            |heights| coarse_corner_heights(heights, grid, tile),
        );
        let corners = projected_corners(&camera, tile, size, heights);
        let color = surface_color(sample);
        let indices = if sample.surface.triangulation == 1 {
            [[0, 1, 3], [1, 2, 3]]
        } else {
            [[0, 1, 2], [0, 2, 3]]
        };
        for (order, indices) in indices.into_iter().enumerate() {
            result.push(ProjectedSurfaceTriangle {
                points: [
                    corners[indices[0]],
                    corners[indices[1]],
                    corners[indices[2]],
                ],
                color,
                tile,
                skirt: false,
                pickable: sample.surface.kind != SceneTerrainSurface::CLIFF,
                order: order as u8,
            });
        }
        if size == 1 {
            append_edge_skirt(
                &mut result,
                &camera,
                tile,
                sample,
                grid_neighbor(&cells, grid, [tile[0] + 1, tile[1]]),
                Edge::East,
            );
            append_edge_skirt(
                &mut result,
                &camera,
                tile,
                sample,
                grid_neighbor(&cells, grid, [tile[0], tile[1] + 1]),
                Edge::South,
            );
        }
    }
    // Painter order is an average-depth approximation. Picking below uses the
    // depth at the hit point, so exact terrain-to-terrain occlusion is still
    // limited until the shared depth-buffer path lands.
    result.sort_unstable_by(|left, right| {
        average_depth(left)
            .total_cmp(&average_depth(right))
            .then(left.tile.cmp(&right.tile))
            .then(left.skirt.cmp(&right.skirt))
            .then(left.order.cmp(&right.order))
    });
    result
}

fn grid_area(min: [i32; 2], max: [i32; 2], size: i32) -> Option<usize> {
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
    (area <= MAX_SURFACE_TILES as i64).then_some(area as usize)
}

fn visible_grid(
    terrain: &[SceneTerrain],
    camera: &Camera,
    viewport: [f64; 2],
    size: i32,
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
        if grid_area(min, max, size).is_none() {
            return Err(());
        }
    }
    if min[0] == i32::MAX {
        return Ok(None);
    }
    let area = grid_area(min, max, size).ok_or(())?;
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

fn fine_tile_visible(
    camera: &Camera,
    sample: SceneTerrain,
    tile: [i32; 2],
    viewport: [f64; 2],
) -> bool {
    let heights = corner_heights(sample.surface.corner_game_height_levels);
    intersects_viewport(&projected_corners(camera, tile, 1, heights), viewport)
}

fn intersects_viewport(points: &[SurfacePoint; 4], viewport: [f64; 2]) -> bool {
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

fn projected_corners(
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

fn corner_heights(heights: [i16; 4]) -> [f64; 4] {
    [
        f64::from(heights[0]),
        f64::from(heights[1]),
        f64::from(heights[2]),
        f64::from(heights[3]),
    ]
}

fn cell_key(tile: [i32; 2], size: i32) -> [i32; 2] {
    [
        tile[0].div_euclid(size) * size,
        tile[1].div_euclid(size) * size,
    ]
}

fn grid_index(grid: CellGrid, tile: [i32; 2]) -> Option<usize> {
    let x = (i64::from(tile[0]) - i64::from(grid.min[0])) / i64::from(grid.size);
    let y = (i64::from(tile[1]) - i64::from(grid.min[1])) / i64::from(grid.size);
    (x >= 0 && y >= 0 && x < grid.width as i64 && y < grid.height as i64)
        .then_some(y as usize * grid.width + x as usize)
}

fn grid_cell(grid: CellGrid, index: usize) -> [i32; 2] {
    [
        grid.min[0] + (index % grid.width) as i32 * grid.size,
        grid.min[1] + (index / grid.width) as i32 * grid.size,
    ]
}

fn cell_center(tile: [i32; 2], size: i32) -> [f64; 2] {
    let offset = f64::from(size) * 0.5;
    [f64::from(tile[0]) + offset, f64::from(tile[1]) + offset]
}

fn grid_neighbor(
    cells: &[Option<CellSample>],
    grid: CellGrid,
    tile: [i32; 2],
) -> Option<&SceneTerrain> {
    let index = grid_index(grid, tile)?;
    cells[index].as_ref().map(|cell| &cell.sample)
}

fn shared_coarse_heights(
    terrain: &[SceneTerrain],
    cells: &[Option<CellSample>],
    grid: CellGrid,
) -> CoarseHeights {
    let vertex_width = grid.width + 1;
    let mut vertices = vec![(0.0, 0_u32); vertex_width * (grid.height + 1)];
    for sample in terrain {
        let tile = tile_key(sample.position);
        let points = cell_vertices(tile, 1);
        for (point, height) in points
            .into_iter()
            .zip(sample.surface.corner_game_height_levels)
        {
            if let Some(index) = coarse_vertex_index(grid, point) {
                vertices[index].0 += f64::from(height);
                vertices[index].1 += 1;
            }
        }
    }
    let mut total = 0.0;
    let mut count = 0_u32;
    for cell in cells.iter().flatten() {
        total += sample_surface_height(
            cell.sample.surface.corner_game_height_levels,
            cell.sample.surface.triangulation,
            0.5,
            0.5,
        );
        count += 1;
    }
    CoarseHeights {
        vertices,
        fallback: total / f64::from(count.max(1)),
    }
}

fn coarse_corner_heights(heights: &CoarseHeights, grid: CellGrid, cell: [i32; 2]) -> [f64; 4] {
    let vertices = cell_vertices(cell, grid.size);
    [
        coarse_height_at(heights, grid, vertices[0]),
        coarse_height_at(heights, grid, vertices[1]),
        coarse_height_at(heights, grid, vertices[2]),
        coarse_height_at(heights, grid, vertices[3]),
    ]
}

fn coarse_height_at(heights: &CoarseHeights, grid: CellGrid, point: [i32; 2]) -> f64 {
    let Some(index) = coarse_vertex_index(grid, point) else {
        return heights.fallback;
    };
    let (sum, count) = heights.vertices[index];
    if count == 0 {
        heights.fallback
    } else {
        sum / f64::from(count)
    }
}

fn coarse_vertex_index(grid: CellGrid, point: [i32; 2]) -> Option<usize> {
    let size = i64::from(grid.size);
    let x = i64::from(point[0]) - i64::from(grid.min[0]);
    let y = i64::from(point[1]) - i64::from(grid.min[1]);
    if x.rem_euclid(size) != 0 || y.rem_euclid(size) != 0 {
        return None;
    }
    let x = x / size;
    let y = y / size;
    if x < 0 || y < 0 || x > grid.width as i64 || y > grid.height as i64 {
        return None;
    }
    Some(y as usize * (grid.width + 1) + x as usize)
}

fn cell_vertices(tile: [i32; 2], size: i32) -> [[i32; 2]; 4] {
    [
        tile,
        [tile[0] + size, tile[1]],
        [tile[0] + size, tile[1] + size],
        [tile[0], tile[1] + size],
    ]
}

fn average_depth(triangle: &ProjectedSurfaceTriangle) -> f64 {
    triangle
        .points
        .iter()
        .map(|point| point.world[0] + point.world[1])
        .sum::<f64>()
        / 3.0
}
