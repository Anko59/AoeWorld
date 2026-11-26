use super::*;

/// Include the broad relief fit, its incident vertex cells and the support
/// ring used to close cliffs. Adjacent patches use the same world samples.
pub fn chunk_surface_halo(size: i32) -> i32 {
    ((20_u32.div_ceil(size as u32) as i32).max(2) + 1) * size
}

/// Build one aligned map chunk, with its surroundings supplying edge data.
/// Faces retain world coordinates and can be projected for any camera.
pub fn chunk_surface_triangles(
    terrain: &[SceneTerrain],
    region: TileRect,
    size: i32,
) -> Vec<ProjectedSurfaceTriangle> {
    let halo = chunk_surface_halo(size);
    let bounds = [
        [region.min.x - halo, region.min.y - halo],
        [region.max.x + halo - 1, region.max.y + halo - 1],
    ];
    let Some(grid) = visible_grid(bounds, size) else {
        return Vec::new();
    };
    let (cells, bank_cells) = sampled_cells(terrain, &vec![true; terrain.len()], grid);
    let (tops, skirts) = budget::surface_counts(&cells, grid);
    let divisions = if size == 1 { 1 } else { 2 };
    let samples = MeshSamples {
        grid,
        cells,
        bank_cells,
        tops,
        skirts,
        divisions,
    };
    mesh_surfaces(
        terrain,
        &Camera::new([0.0; 2], [0.0; 2]),
        samples,
        Some(region),
    )
}
