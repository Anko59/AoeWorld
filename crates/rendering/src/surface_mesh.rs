#[cfg(test)]
use crate::GAME_ATLAS_SIDE;
use crate::{AtlasAddress, GameArt, GameFrame, SceneCamera, SceneTerrain, SceneTerrainSurface};
use aoe_core::{Camera, ScreenPoint};
#[cfg(test)]
use web_sys::CanvasRenderingContext2d;

mod appearance;
mod cliff_art;
pub(crate) mod directory;
pub(crate) mod floor;
pub(crate) mod landscape;
pub(crate) use appearance::{apply_terrain_textures, procedural_tint, terrain_texture_frame};
mod lod;
mod picking;
pub use picking::pick_surface_point;
const SCULPTED_ORDER: u8 = 128;
/// Ground order bit: the colour channels carry `relief` codes.
const RELIEF_FLAG: u8 = 64;
mod skirts;
use skirts::{Edge, SkirtSurface, append_edge_skirt};
pub(crate) mod mix;
pub(crate) mod relief;
pub use directory::write_terrain_directory;
pub use lod::projected_surface_triangles;

#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "surface_mesh/tests/water_fixture.rs"]
pub(crate) mod water_fixture;

/// Quantization of footprint wear shared by the packets, shaders and Canvas.
pub(crate) const WEAR_LEVELS: u8 = 15;
/// Per-channel darkening at full wear (red, green, blue).
pub(crate) const WEAR_DARKEN: [f32; 3] = [0.20, 0.32, 0.38];

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
    /// Packed optional metadata; zero means none, presence bit preserves Some(0).
    pub(crate) appearance: u32,
    /// Three canonical rounded u8 endpoints; Some opts into floor interpolation.
    pub(crate) floor_strengths: Option<[u8; 3]>,
    /// Overlay then middle shares per corner (`mix`); all zero means unmixed.
    pub(crate) mix: [u8; 6],
    /// Bits 0-2 pick the texture mode; bits 3-6 hold footprint wear (`wear`). Read
    /// them through `mode`/`wear`: the packed size is pinned, so no spare byte exists.
    pub(crate) texture_mode: u8,
    pub(crate) tint: u8,
    pub(crate) texture_uv: Option<AtlasAddress>,
    /// Junction mode reuses the third rectangle for three packed observations.
    pub(crate) texture_blend: Option<[AtlasAddress; 2]>,
    pub(crate) texture_tile: [i32; 2],
    pub(crate) texture_materials: Option<[u8; 3]>,
    pub(crate) pickable: bool,
    /// Ordinary draw order, or bit seven plus source span and triangulation.
    pub(crate) order: u8,
}

impl ProjectedSurfaceTriangle {
    pub(crate) fn mode(&self) -> u8 {
        self.texture_mode & 7
    }

    /// Building footprint wear, 0..=`WEAR_LEVELS`; textured ground ignores `color`.
    pub(crate) fn wear(&self) -> u8 {
        (self.texture_mode >> 3) & WEAR_LEVELS
    }

    pub(crate) fn set_wear(&mut self, wear: u8) {
        self.texture_mode = self.mode() | wear.min(WEAR_LEVELS) << 3;
    }

    pub(crate) fn draw_order(&self) -> u8 {
        if self.order & SCULPTED_ORDER != 0 {
            self.order & 1
        } else if self.skirt {
            self.order
        } else {
            self.order & 3
        }
    }

    /// Per-vertex slope shade codes (`relief`): the low four mantissa bits of
    /// the three `color` channels (invisible in an untextured fill, ignored by
    /// textured ground), valid only while `order` carries `RELIEF_FLAG`, so the
    /// packed size stays 240 bytes. Sculpted cliff pieces and skirts have none.
    pub(crate) fn relief(&self) -> [i8; 3] {
        if self.skirt || self.order & (SCULPTED_ORDER | RELIEF_FLAG) != RELIEF_FLAG {
            return [0; 3];
        }
        self.color.map(|c| (((c.to_bits() as u8) << 4) as i8) >> 4)
    }

    /// Multiplies the fill colour, keeping the slope shade codes it carries.
    pub(crate) fn scale_color(&mut self, factors: [f32; 3]) {
        let relief = self.relief();
        let shaded = self.order & (SCULPTED_ORDER | RELIEF_FLAG) == RELIEF_FLAG;
        for (channel, factor) in self.color.iter_mut().zip(factors) {
            *channel *= factor;
        }
        if shaded && !self.skirt {
            self.set_relief(relief);
        }
    }

    pub(crate) fn set_relief(&mut self, codes: [i8; 3]) {
        if self.skirt || self.order & SCULPTED_ORDER != 0 {
            return;
        }
        for (channel, code) in self.color.iter_mut().zip(codes) {
            *channel = f32::from_bits((channel.to_bits() & !15) | u32::from(code as u8 & 15));
        }
        self.order |= RELIEF_FLAG;
    }
}

pub fn sample_surface_height(
    corners: [i16; 4],
    triangulation: u8,
    local_x: f64,
    local_y: f64,
) -> f64 {
    sample_float_surface_height(
        [
            f64::from(corners[0]),
            f64::from(corners[1]),
            f64::from(corners[2]),
            f64::from(corners[3]),
        ],
        triangulation,
        local_x,
        local_y,
    )
}

fn sample_float_surface_height(
    corners: [f64; 4],
    triangulation: u8,
    local_x: f64,
    local_y: f64,
) -> f64 {
    let [northwest, northeast, southeast, southwest] = corners;
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

pub fn surface_depth_at(
    triangles: &[ProjectedSurfaceTriangle],
    screen: ScreenPoint,
) -> Option<f64> {
    surface_hit_at(triangles, screen).map(|hit| hit.0)
}

/// Frontmost visible terrain depth and whether its surface accepts a ground
/// target. Cliff faces remain visible blockers even when order picking sees
/// eligible ground behind them.
pub fn surface_hit_at(
    triangles: &[ProjectedSurfaceTriangle],
    screen: ScreenPoint,
) -> Option<(f64, bool)> {
    frontmost_surface(triangles, screen).map(|(depth, skirt, pickable)| (depth, !skirt && pickable))
}

/// Whether the frontmost drawn surface under `screen` is a cliff face nearer
/// the viewer than an object at `depth`, so the object is hidden there.
pub fn cliff_face_in_front(
    triangles: &[ProjectedSurfaceTriangle],
    screen: ScreenPoint,
    depth: f64,
) -> bool {
    frontmost_surface(triangles, screen).is_some_and(|(front, skirt, _)| skirt && front > depth)
}

/// Frontmost drawn depth under a pixel, with whether it is a cliff face and
/// whether the triangle accepts ground picking.
fn frontmost_surface(
    triangles: &[ProjectedSurfaceTriangle],
    screen: ScreenPoint,
) -> Option<(f64, bool, bool)> {
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
            Some((
                surface_render_depth(world, triangle.skirt),
                triangle.skirt,
                triangle.pickable,
            ))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
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

pub(crate) fn triangle_texture_coordinates(mode: u8) -> [[f64; 2]; 3] {
    match mode {
        0 => [[0.5, 0.0], [1.0, 0.5], [0.5, 1.0]],
        1 => [[0.5, 0.0], [0.5, 1.0], [0.0, 0.5]],
        2 => [[0.5, 0.0], [1.0, 0.5], [0.0, 0.5]],
        3 => [[1.0, 0.5], [0.5, 1.0], [0.0, 0.5]],
        4 => [[0.3, 0.3], [0.7, 0.3], [0.7, 0.7]],
        5 => [[0.3, 0.3], [0.7, 0.7], [0.3, 0.7]],
        _ => [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
    }
}

#[cfg(test)]
pub(crate) fn draw_surface_triangle(
    context: &CanvasRenderingContext2d,
    atlases: &[web_sys::HtmlCanvasElement; 5],
    triangle: &ProjectedSurfaceTriangle,
) -> Result<(), String> {
    let uv = triangle_texture_coordinates(triangle.mode());
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
            // This retained tint oracle is intentionally a single-page fixture.
            assert_eq!(rect.page, 0);
            let [x, y, width, height] = rect.uv.map(f64::from);
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
    for texel in tinted.as_chunks_mut::<4>().0 {
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

fn surface_color(sample: SceneTerrain) -> [f32; 3] {
    let mut color = match sample.material {
        1 => [0.56, 0.48, 0.28],
        2 | 17 => [0.48, 0.30, 0.18],
        3 => [0.74, 0.66, 0.42],
        4 => [0.40, 0.40, 0.40],
        5 => [0.22, 0.46, 0.66],
        7 => [0.84, 0.85, 0.84],
        8 => [0.57, 0.70, 0.75],
        9 => [0.28, 0.24, 0.19],
        10 => [0.34, 0.48, 0.45],
        _ => [0.28, 0.50, 0.23],
    };
    if sample.surface.water != 0 {
        color = [0.18, 0.42, 0.66];
    }
    if sample.surface.kind == SceneTerrainSurface::RAMP {
        color = darken(color, 0.88);
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
