//! Closed terrain edges, sharing the exact displayed top and bottom vertices.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum Edge {
    East,
    South,
}

#[derive(Clone, Copy)]
pub(super) struct SkirtSurface {
    pub(super) sample: SceneTerrain,
    pub(super) heights: [f64; 4],
}

pub(super) fn append_edge_skirt(
    result: &mut Vec<ProjectedSurfaceTriangle>,
    projection: &Camera,
    tile: [i32; 2],
    size: i32,
    current: SkirtSurface,
    neighbor: Option<SkirtSurface>,
    edge: Edge,
) {
    let Some(neighbor) = neighbor else {
        return;
    };
    let sample = current.sample;
    let adjacent_sample = neighbor.sample;
    // Modeled water has an independent level from the terrain underneath it.
    // Its banks (and downstream level changes) need the same closed vertical
    // seam as cliffs, even when both adjacent top surfaces are plateaus.
    if sample.surface.water == 0
        && adjacent_sample.surface.water == 0
        && !sample.surface.is_cliff()
        && !adjacent_sample.surface.is_cliff()
    {
        return;
    }
    let current = current.heights;
    let adjacent = neighbor.heights;
    let (current_indices, neighbor_indices) = match edge {
        Edge::East => ([1, 2], [0, 3]),
        Edge::South => ([2, 3], [1, 0]),
    };
    if current_indices
        .iter()
        .zip(neighbor_indices)
        .all(|(current_index, neighbor_index)| current[*current_index] == adjacent[neighbor_index])
    {
        return;
    }
    let mut top = [0.0; 2];
    let mut bottom = [0.0; 2];
    for index in 0..2 {
        top[index] = current[current_indices[index]].max(adjacent[neighbor_indices[index]]);
        bottom[index] = current[current_indices[index]].min(adjacent[neighbor_indices[index]]);
    }
    let world_xy = match edge {
        Edge::East => [
            [f64::from(tile[0] + size), f64::from(tile[1])],
            [f64::from(tile[0] + size), f64::from(tile[1] + size)],
        ],
        Edge::South => [
            [f64::from(tile[0] + size), f64::from(tile[1] + size)],
            [f64::from(tile[0]), f64::from(tile[1] + size)],
        ],
    };
    let points = [
        project_world(projection, world_xy[0], top[0]),
        project_world(projection, world_xy[1], top[1]),
        project_world(projection, world_xy[1], bottom[1]),
        project_world(projection, world_xy[0], bottom[0]),
    ];
    let bank = if sample.surface.water != 0 && adjacent_sample.surface.water == 0 {
        adjacent_sample
    } else {
        sample
    };
    let water_step = sample.surface.water != 0 && adjacent_sample.surface.water != 0;
    let cliff = sample.surface.water == 0
        && adjacent_sample.surface.water == 0
        && (sample.surface.is_cliff() || adjacent_sample.surface.is_cliff());
    // Private pre-texturing marker: native faces replace it with tint zero;
    // missing art follows the existing procedural rock fallback.
    let shore = (sample.surface.water != 0) != (adjacent_sample.surface.water != 0);
    let material = if water_step {
        5
    } else if shore && !bank.surface.is_cliff() {
        bank.material
    } else {
        4
    };
    let gentle = shore && material != 4 && top.iter().zip(bottom).all(|(a, b)| a - b <= 1.0);
    let tint = if cliff {
        13
    } else if water_step || gentle {
        0
    } else {
        3
    };
    let color = darken(surface_color(bank), if gentle { 1.0 } else { 0.62 });
    result.push(ProjectedSurfaceTriangle {
        appearance: landscape::pack(bank.appearance),
        floor_strengths: None,
        mix: [0; 6],
        points: [points[0], points[1], points[2]],
        color,
        tile,
        skirt: true,
        material,
        texture_mode: 4,
        tint,
        texture_uv: None,
        texture_blend: None,
        texture_tile: tile,
        texture_materials: gentle.then_some([material; 3]),
        pickable: false,
        order: skirt_order(edge),
    });
    result.push(ProjectedSurfaceTriangle {
        appearance: landscape::pack(bank.appearance),
        floor_strengths: None,
        mix: [0; 6],
        points: [points[0], points[2], points[3]],
        color,
        tile,
        skirt: true,
        material,
        texture_mode: 5,
        tint,
        texture_uv: None,
        texture_blend: None,
        texture_tile: tile,
        texture_materials: gentle.then_some([material; 3]),
        pickable: false,
        order: skirt_order(edge) + 1,
    });
}

fn skirt_order(edge: Edge) -> u8 {
    match edge {
        Edge::East => 2,
        Edge::South => 4,
    }
}
