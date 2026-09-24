use crate::{SceneCamera, SceneTerrain, SceneTerrainSurface};
use aoe_core::{Camera, ScreenPoint};
use web_sys::CanvasRenderingContext2d;

mod lod;
pub use lod::projected_surface_triangles;

pub const MAX_SURFACE_TILES: usize = 4_096;
pub const MAX_SURFACE_TRIANGLES: usize = MAX_SURFACE_TILES * 6;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

#[derive(Clone, Copy, Debug)]
pub struct SurfacePoint {
    pub world: [f64; 3],
    pub screen: ScreenPoint,
}

#[derive(Clone, Copy, Debug)]
pub struct ProjectedSurfaceTriangle {
    pub points: [SurfacePoint; 3],
    pub color: [f32; 3],
    pub tile: [i32; 2],
    pub skirt: bool,
    pickable: bool,
    order: u8,
}

pub fn sample_surface_height(
    corners: [i16; 4],
    triangulation: u8,
    local_x: f64,
    local_y: f64,
) -> f64 {
    let northwest = f64::from(corners[0]);
    let northeast = f64::from(corners[1]);
    let southeast = f64::from(corners[2]);
    let southwest = f64::from(corners[3]);
    if triangulation == 0 {
        if local_x >= local_y {
            (1.0 - local_x) * northwest + (local_x - local_y) * northeast + local_y * southeast
        } else {
            (1.0 - local_y) * northwest + local_x * southeast + (local_y - local_x) * southwest
        }
    } else if local_x + local_y <= 1.0 {
        (1.0 - local_x - local_y) * northwest + local_x * northeast + local_y * southwest
    } else {
        (1.0 - local_y) * northeast
            + (local_x + local_y - 1.0) * southeast
            + (1.0 - local_x) * southwest
    }
}

pub fn pick_surface_point(
    triangles: &[ProjectedSurfaceTriangle],
    screen: ScreenPoint,
) -> Option<[f64; 2]> {
    let mut best: Option<(f64, [i32; 2], bool, bool, [f64; 2])> = None;
    for triangle in triangles {
        let Some(weights) = barycentric(triangle.points, screen) else {
            continue;
        };
        let world = [
            triangle.points[0].world[0] * weights[0]
                + triangle.points[1].world[0] * weights[1]
                + triangle.points[2].world[0] * weights[2],
            triangle.points[0].world[1] * weights[0]
                + triangle.points[1].world[1] * weights[1]
                + triangle.points[2].world[1] * weights[2],
        ];
        let hit = (
            world[0] + world[1],
            triangle.tile,
            triangle.skirt,
            triangle.pickable,
            world,
        );
        if best.as_ref().is_none_or(|current| {
            hit.0
                .total_cmp(&current.0)
                .then(hit.1.cmp(&current.1))
                .then(hit.2.cmp(&current.2))
                .is_gt()
        }) {
            best = Some(hit);
        }
    }
    best.and_then(|(_, _, skirt, pickable, world)| (!skirt && pickable).then_some(world))
}

pub fn draw_surface_mesh(
    context: &CanvasRenderingContext2d,
    triangles: &[ProjectedSurfaceTriangle],
) -> Result<(), String> {
    for triangle in triangles {
        let [r, g, b] = triangle.color;
        let color = [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8];
        let style = [
            b'#',
            HEX_DIGITS[usize::from(color[0] >> 4)],
            HEX_DIGITS[usize::from(color[0] & 0x0f)],
            HEX_DIGITS[usize::from(color[1] >> 4)],
            HEX_DIGITS[usize::from(color[1] & 0x0f)],
            HEX_DIGITS[usize::from(color[2] >> 4)],
            HEX_DIGITS[usize::from(color[2] & 0x0f)],
        ];
        context.set_fill_style_str(std::str::from_utf8(&style).unwrap_or("#000000"));
        context.begin_path();
        let first = triangle.points[0].screen;
        context.move_to(first.x, first.y);
        for point in triangle.points.iter().skip(1) {
            context.line_to(point.screen.x, point.screen.y);
        }
        context.close_path();
        context.fill();
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Edge {
    East,
    South,
}

fn append_edge_skirt(
    result: &mut Vec<ProjectedSurfaceTriangle>,
    projection: &Camera,
    tile: [i32; 2],
    sample: SceneTerrain,
    neighbor: Option<&SceneTerrain>,
    edge: Edge,
) {
    let Some(neighbor) = neighbor else {
        return;
    };
    if sample.surface.water != 0 || neighbor.surface.water != 0 {
        return;
    }
    if sample.surface.kind != SceneTerrainSurface::CLIFF
        && neighbor.surface.kind != SceneTerrainSurface::CLIFF
    {
        return;
    }
    let current = effective_heights(sample);
    let adjacent = effective_heights(*neighbor);
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
        top[index] =
            f64::from(current[current_indices[index]].max(adjacent[neighbor_indices[index]]));
        bottom[index] =
            f64::from(current[current_indices[index]].min(adjacent[neighbor_indices[index]]));
    }
    let world_xy = match edge {
        Edge::East => [
            [f64::from(tile[0] + 1), f64::from(tile[1])],
            [f64::from(tile[0] + 1), f64::from(tile[1] + 1)],
        ],
        Edge::South => [
            [f64::from(tile[0] + 1), f64::from(tile[1] + 1)],
            [f64::from(tile[0]), f64::from(tile[1] + 1)],
        ],
    };
    let points = [
        project_world(projection, world_xy[0], top[0]),
        project_world(projection, world_xy[1], top[1]),
        project_world(projection, world_xy[1], bottom[1]),
        project_world(projection, world_xy[0], bottom[0]),
    ];
    let color = darken(surface_color(sample), 0.62);
    result.push(ProjectedSurfaceTriangle {
        points: [points[0], points[1], points[2]],
        color,
        tile,
        skirt: true,
        pickable: false,
        order: skirt_order(edge),
    });
    result.push(ProjectedSurfaceTriangle {
        points: [points[0], points[2], points[3]],
        color,
        tile,
        skirt: true,
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

fn project_world(projection: &Camera, world_xy: [f64; 2], elevation: f64) -> SurfacePoint {
    SurfacePoint {
        world: [world_xy[0], world_xy[1], elevation],
        screen: projection.world_to_screen_at_height(world_xy, elevation),
    }
}

fn tile_key(position: [f64; 2]) -> [i32; 2] {
    [
        (position[0] - 0.5).round() as i32,
        (position[1] - 0.5).round() as i32,
    ]
}

fn effective_heights(sample: SceneTerrain) -> [i16; 4] {
    sample.surface.corner_game_height_levels
}

fn surface_color(sample: SceneTerrain) -> [f32; 3] {
    let mut color = match sample.material {
        1 => [0.56, 0.48, 0.28],
        2 => [0.48, 0.30, 0.18],
        3 => [0.74, 0.66, 0.42],
        4 => [0.40, 0.40, 0.40],
        5 => [0.22, 0.46, 0.66],
        _ => [0.28, 0.50, 0.23],
    };
    if sample.surface.water != 0 {
        color = [0.18, 0.42, 0.66];
    }
    if sample.surface.kind == SceneTerrainSurface::RAMP {
        color = darken(color, 0.88);
    }
    if sample.surface.kind == SceneTerrainSurface::CLIFF {
        color = darken(color, 0.78);
    }
    color
}

fn darken(color: [f32; 3], factor: f32) -> [f32; 3] {
    [color[0] * factor, color[1] * factor, color[2] * factor]
}

pub(crate) fn barycentric(points: [SurfacePoint; 3], screen: ScreenPoint) -> Option<[f64; 3]> {
    let a = points[0].screen;
    let b = points[1].screen;
    let c = points[2].screen;
    let denominator = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
    if denominator.abs() < f64::EPSILON {
        return None;
    }
    let first = ((b.y - c.y) * (screen.x - c.x) + (c.x - b.x) * (screen.y - c.y)) / denominator;
    let second = ((c.y - a.y) * (screen.x - c.x) + (a.x - c.x) * (screen.y - c.y)) / denominator;
    let third = 1.0 - first - second;
    (first >= -1e-7 && second >= -1e-7 && third >= -1e-7).then_some([first, second, third])
}
