#[cfg(test)]
use crate::GAME_ATLAS_SIDE;
use crate::{GameArt, GameFrame, SceneCamera, SceneTerrain, SceneTerrainSurface};
use aoe_core::{Camera, ScreenPoint};
#[cfg(test)]
use web_sys::CanvasRenderingContext2d;

mod lod;
pub use lod::projected_surface_triangles;

#[cfg(test)]
mod tests;

pub const MAX_SURFACE_TILES: usize = 4_096;
pub const MAX_SURFACE_TRIANGLES: usize = MAX_SURFACE_TILES * 6;
#[cfg(test)]
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
    pub(crate) material: u8,
    pub(crate) texture_mode: u8,
    pub(crate) tint: u8,
    pub(crate) texture_uv: Option<[f32; 4]>,
    pub(crate) pickable: bool,
    pub(crate) order: u8,
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
        let elevation = triangle.points[0].world[2] * weights[0]
            + triangle.points[1].world[2] * weights[1]
            + triangle.points[2].world[2] * weights[2];
        let hit = (
            surface_render_depth([world[0], world[1], elevation], triangle.skirt),
            triangle.tile,
            triangle.skirt,
            triangle.pickable,
            world,
        );
        if best.as_ref().is_none_or(|current| {
            hit.0
                .total_cmp(&current.0)
                .then((!hit.2).cmp(&(!current.2)))
                .then(hit.3.cmp(&current.3))
                .then(hit.1.cmp(&current.1))
                .is_gt()
        }) {
            best = Some(hit);
        }
    }
    best.and_then(|(_, _, skirt, pickable, world)| (!skirt && pickable).then_some(world))
}

pub fn surface_depth_at(
    triangles: &[ProjectedSurfaceTriangle],
    screen: ScreenPoint,
) -> Option<f64> {
    triangles
        .iter()
        .filter_map(|triangle| {
            let weights = barycentric(triangle.points, screen)?;
            let world = [
                triangle.points[0].world[0] * weights[0]
                    + triangle.points[1].world[0] * weights[1]
                    + triangle.points[2].world[0] * weights[2],
                triangle.points[0].world[1] * weights[0]
                    + triangle.points[1].world[1] * weights[1]
                    + triangle.points[2].world[1] * weights[2],
                triangle.points[0].world[2] * weights[0]
                    + triangle.points[1].world[2] * weights[1]
                    + triangle.points[2].world[2] * weights[2],
            ];
            Some(surface_render_depth(world, triangle.skirt))
        })
        .max_by(f64::total_cmp)
}

pub(crate) fn surface_depth(world: [f64; 3]) -> f64 {
    world[0] + world[1] + 2.0 * world[2]
}

/// Shared cliff faces lose an exact depth tie to the terrain surface, matching
/// picking's non-skirt preference and preventing a coplanar skirt from covering
/// the walkable top at the depth buffer's equal comparison.
pub(crate) fn surface_render_depth(world: [f64; 3], skirt: bool) -> f64 {
    surface_depth(world) - if skirt { 0.01 } else { 0.0 }
}

pub(crate) fn apply_terrain_textures(triangles: &mut [ProjectedSurfaceTriangle], art: &GameArt) {
    for triangle in triangles {
        let frame = terrain_texture_frame(art, triangle.material, triangle.tile);
        triangle.texture_uv = frame.map(|frame| frame.uv);
    }
}

fn terrain_texture_frame(art: &GameArt, material: u8, tile: [i32; 2]) -> Option<GameFrame> {
    let frames = art
        .terrain
        .get(usize::from(material))
        .filter(|frames| !frames.is_empty())
        .unwrap_or(&art.grass);
    if frames.is_empty() {
        return None;
    }
    let index = (tile[0]
        .wrapping_mul(7)
        .wrapping_add(tile[1].wrapping_mul(13))
        .unsigned_abs() as usize)
        % frames.len();
    frames.get(index).copied()
}

pub(crate) fn triangle_texture_coordinates(mode: u8) -> [[f64; 2]; 3] {
    match mode {
        0 => [[0.5, 0.0], [1.0, 0.5], [0.5, 1.0]],
        1 => [[0.5, 0.0], [0.5, 1.0], [0.0, 0.5]],
        2 => [[0.5, 0.0], [1.0, 0.5], [0.0, 0.5]],
        3 => [[1.0, 0.5], [0.5, 1.0], [0.0, 0.5]],
        4 => [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        5 => [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        _ => [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
    }
}

#[cfg(test)]
pub(crate) fn draw_surface_triangle(
    context: &CanvasRenderingContext2d,
    atlases: &[web_sys::HtmlCanvasElement; 5],
    triangle: &ProjectedSurfaceTriangle,
) -> Result<(), String> {
    let uv = triangle_texture_coordinates(triangle.texture_mode);
    context.save();
    context.begin_path();
    context.move_to(triangle.points[0].screen.x, triangle.points[0].screen.y);
    for point in triangle.points.iter().skip(1) {
        context.line_to(point.screen.x, point.screen.y);
    }
    context.close_path();
    let draw_result = (|| {
        if let Some(rect) = triangle.texture_uv {
            context.clip();
            let transform = texture_transform(triangle.points, uv);
            context
                .set_transform(
                    transform[0],
                    transform[1],
                    transform[2],
                    transform[3],
                    transform[4],
                    transform[5],
                )
                .map_err(|error| format!("Canvas terrain transform: {error:?}"))?;
            let [x, y, width, height] = rect.map(f64::from);
            let atlas_side = f64::from(GAME_ATLAS_SIDE);
            let texture_atlas = atlases
                .get(usize::from(triangle.tint))
                .unwrap_or(&atlases[0]);
            context
                .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    texture_atlas,
                    x * atlas_side,
                    y * atlas_side,
                    width * atlas_side,
                    height * atlas_side,
                    0.0,
                    0.0,
                    1.0,
                    1.0,
                )
                .map_err(|error| format!("Canvas terrain texture: {error:?}"))?;
        } else {
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
            context.fill();
        }
        Ok(())
    })();
    context.restore();
    draw_result
}

#[cfg(test)]
pub(crate) fn texture_transform(points: [SurfacePoint; 3], uv: [[f64; 2]; 3]) -> [f64; 6] {
    let [u0, v0] = uv[0];
    let du1 = uv[1][0] - u0;
    let dv1 = uv[1][1] - v0;
    let du2 = uv[2][0] - u0;
    let dv2 = uv[2][1] - v0;
    let determinant = du1 * dv2 - du2 * dv1;
    if determinant.abs() < f64::EPSILON {
        return [0.0; 6];
    }
    let dx1 = points[1].screen.x - points[0].screen.x;
    let dx2 = points[2].screen.x - points[0].screen.x;
    let dy1 = points[1].screen.y - points[0].screen.y;
    let dy2 = points[2].screen.y - points[0].screen.y;
    let a = (dx1 * dv2 - dx2 * dv1) / determinant;
    let c = (du1 * dx2 - du2 * dx1) / determinant;
    let b = (dy1 * dv2 - dy2 * dv1) / determinant;
    let d = (du1 * dy2 - du2 * dy1) / determinant;
    [
        a,
        b,
        c,
        d,
        points[0].screen.x - a * u0 - c * v0,
        points[0].screen.y - b * u0 - d * v0,
    ]
}

#[cfg(test)]
pub(crate) fn tint_atlas_pixels(pixels: &[u8], tint: u8) -> Option<Vec<u8>> {
    if pixels.len() % 4 != 0 {
        return None;
    }
    let mut tinted = pixels.to_vec();
    for texel in tinted.chunks_exact_mut(4) {
        match tint {
            1 | 2 | 3 => {
                let factor = [0.92, 0.78, 0.72][usize::from(tint - 1)];
                for channel in &mut texel[..3] {
                    *channel = (f32::from(*channel) * factor).round() as u8;
                }
            }
            4 => {
                for (channel, water) in texel[..3].iter_mut().zip([38_u8, 113, 190]) {
                    *channel = (f32::from(*channel) * 0.86 + f32::from(water) * 0.14).round() as u8;
                }
            }
            _ => {}
        }
    }
    Some(tinted)
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
        material: 4,
        texture_mode: 4,
        tint: 3,
        texture_uv: None,
        pickable: false,
        order: skirt_order(edge),
    });
    result.push(ProjectedSurfaceTriangle {
        points: [points[0], points[2], points[3]],
        color,
        tile,
        skirt: true,
        material: 4,
        texture_mode: 5,
        tint: 3,
        texture_uv: None,
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
