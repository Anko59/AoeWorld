use crate::{GAME_ATLAS_SIDE, SceneCamera, surface_mesh::surface_depth, web::Sprite};
use aoe_core::{Camera, MAX_WORLD_DIMENSION_TILES, ScreenPoint, TileRect, WorldConfig};
use web_sys::CanvasRenderingContext2d;

const GRID_COLOR: [f32; 4] = [31.0 / 255.0, 38.0 / 255.0, 31.0 / 255.0, 0.2];
const GRID_STROKE: &str = "rgba(31,38,31,.2)";
const GRID_WIDTH: f64 = 1.0;
pub(crate) const SELECTION_RING_SPRITES: usize = 32;

pub(crate) fn selection_ring(
    camera: SceneCamera,
    position: [f64; 2],
    elevation_meters: f64,
) -> Vec<(Sprite, f64)> {
    let screen = camera_projection(camera).world_to_screen_at_height(position, elevation_meters);
    let radius_x = 24.0 * camera.zoom;
    let radius_y = 8.0 * camera.zoom;
    let mut sprites = Vec::with_capacity(SELECTION_RING_SPRITES);
    for [cos, sin] in RING_POINTS {
        let point = ScreenPoint {
            x: screen.x + cos * radius_x,
            y: screen.y + sin * radius_y,
        };
        let world = camera_projection(camera).screen_to_world_at_height(point, elevation_meters);
        sprites.push((
            Sprite {
                position: to_clip(point, camera.viewport),
                radius: pixel_radius(camera.viewport, 1.5),
                color: [0.95, 0.85, 0.35, 1.0],
                uv: solid_uv(),
                depths: [0.0; 4],
                terrain_blend: [[0.0; 4]; 2],
                pages: [2, 0, 0, 0],
            },
            surface_depth([world[0], world[1], elevation_meters]),
        ));
    }
    sprites
}

pub(crate) fn viewport_bounds(camera: SceneCamera) -> TileRect {
    let config = WorldConfig {
        width_tiles: MAX_WORLD_DIMENSION_TILES,
        height_tiles: MAX_WORLD_DIMENSION_TILES,
        ..WorldConfig::default()
    };
    camera_projection(camera).visible_tiles_at_height(config, 1.0, camera.focus_elevation_meters)
}

pub(crate) fn grid_sprites(camera: SceneCamera, bounds: TileRect) -> Vec<Sprite> {
    let lines = grid_lines(camera, bounds);
    let mut sprites = Vec::with_capacity(lines.len() * 2);
    for (start, end) in lines {
        add_line(&mut sprites, start, end, camera.viewport);
    }
    sprites
}

pub(crate) fn draw_grid(
    context: &CanvasRenderingContext2d,
    backing: [u32; 2],
    camera: SceneCamera,
    bounds: TileRect,
) {
    // The renderer already owns the backing dimensions; avoid a DOM lookup on
    // every frame and keep the CSS/backing relationship explicit for tests.
    let scale = [
        f64::from(backing[0]) / camera.viewport[0],
        f64::from(backing[1]) / camera.viewport[1],
    ];
    if scale
        .into_iter()
        .any(|value| !value.is_finite() || value <= 0.0)
    {
        return;
    }
    let lines = grid_lines(camera, bounds);
    if lines.is_empty() {
        return;
    }
    context.save();
    if context
        .set_transform(scale[0], 0.0, 0.0, scale[1], 0.0, 0.0)
        .is_err()
    {
        context.restore();
        return;
    }
    context.set_stroke_style_str(GRID_STROKE);
    context.set_line_width(GRID_WIDTH);
    context.set_line_cap("butt");
    // Independently composite each of the <=256 strips, like the GPU packets.
    // A combined path unions crossings and would apply alpha only once there.
    for (start, end) in lines {
        context.begin_path();
        context.move_to(start.x, start.y);
        context.line_to(end.x, end.y);
        context.stroke();
    }
    context.restore();
}

fn camera_projection(camera: SceneCamera) -> Camera {
    Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    }
}

#[inline(never)]
fn grid_lines(camera: SceneCamera, visible: TileRect) -> Vec<(ScreenPoint, ScreenPoint)> {
    if !camera.zoom.is_finite()
        || camera.zoom <= 0.0
        || !camera.focus_elevation_meters.is_finite()
        || camera.center.into_iter().any(|value| !value.is_finite())
        || camera
            .viewport
            .into_iter()
            .any(|value| !value.is_finite() || value <= 0.0)
        || visible.min.x >= visible.max.x
        || visible.min.y >= visible.max.y
    {
        return Vec::new();
    }
    let projection = camera_projection(camera);
    let min_x = visible.min.x;
    let max_x = visible.max.x;
    let min_y = visible.min.y;
    let max_y = visible.max.y;
    // Perpendicular separation of projected world-coordinate lines, not distance
    // along either diagonal. A single power-of-two step keeps both axes aligned
    // to absolute world integer multiples, independent of the visible minimum.
    let spacing = aoe_core::ISO_TILE_WIDTH * aoe_core::ISO_TILE_HEIGHT
        / (aoe_core::ISO_TILE_WIDTH * aoe_core::ISO_TILE_WIDTH
            + aoe_core::ISO_TILE_HEIGHT * aoe_core::ISO_TILE_HEIGHT)
            .sqrt()
        * camera.zoom;
    if !spacing.is_finite() || spacing <= 0.0 {
        return Vec::new();
    }
    let extent = (i64::from(max_x) - i64::from(min_x)).max(i64::from(max_y) - i64::from(min_y));
    let mut step = 1_i64;
    while spacing * (step as f64) < 24.0 || extent / step + 1 > 128 {
        if step > i64::MAX / 2 {
            return Vec::new();
        }
        step *= 2;
    }
    let min = [min_x, min_y];
    let max = [max_x, max_y];
    let mut lines = Vec::new();
    for axis in 0..2 {
        // Exact signed ceil-to-multiple: step is a positive power of two.
        // i32 endpoints plus step <= 2^62 cannot overflow this i64 sum.
        let first = (i64::from(min[axis]) + step - 1) & -step;
        for index in 0..128 {
            let coordinate = first + i64::from(index) * step;
            if coordinate > i64::from(max[axis]) {
                break;
            }
            let mut start = min.map(f64::from);
            let mut end = max.map(f64::from);
            start[axis] = coordinate as f64;
            end[axis] = coordinate as f64;
            add_clipped_line(
                &mut lines,
                projection.world_to_screen_at_height(start, camera.focus_elevation_meters),
                projection.world_to_screen_at_height(end, camera.focus_elevation_meters),
                camera.viewport,
            );
        }
    }
    lines
}

fn add_clipped_line(
    lines: &mut Vec<(ScreenPoint, ScreenPoint)>,
    start: ScreenPoint,
    end: ScreenPoint,
    viewport: [f64; 2],
) {
    if let Some(line) = clip_line(start, end, viewport) {
        lines.push(line);
    }
}

fn clip_line(
    start: ScreenPoint,
    end: ScreenPoint,
    viewport: [f64; 2],
) -> Option<(ScreenPoint, ScreenPoint)> {
    if viewport
        .into_iter()
        .any(|value| !value.is_finite() || value <= 0.0)
        || [start.x, start.y, end.x, end.y]
            .into_iter()
            .any(|value| !value.is_finite())
    {
        return None;
    }
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0) {
        return None;
    }
    let mut first: f64 = 0.0;
    let mut last: f64 = 1.0;
    for (p, q) in [
        (-dx, start.x),
        (dx, viewport[0] - start.x),
        (-dy, start.y),
        (dy, viewport[1] - start.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let ratio = q / p;
        if p < 0.0 {
            if ratio > last {
                return None;
            }
            first = first.max(ratio);
        } else {
            if ratio < first {
                return None;
            }
            last = last.min(ratio);
        }
    }
    if first >= last {
        return None;
    }
    Some((
        ScreenPoint {
            x: start.x + dx * first,
            y: start.y + dy * first,
        },
        ScreenPoint {
            x: start.x + dx * last,
            y: start.y + dy * last,
        },
    ))
}

fn add_line(sprites: &mut Vec<Sprite>, start: ScreenPoint, end: ScreenPoint, viewport: [f64; 2]) {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    // Normalize before squaring: finite extreme or subnormal segments cannot
    // overflow/underflow their length, and no general-purpose hypot is needed.
    let scale = dx.abs().max(dy.abs());
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let [x, y] = [dx / scale, dy / scale];
    let radius = GRID_WIDTH * 0.5 / (x * x + y * y).sqrt();
    let normal = [-y * radius, x * radius];
    let edge = |point: ScreenPoint, sign: f64| {
        to_clip(
            ScreenPoint {
                x: point.x + normal[0] * sign,
                y: point.y + normal[1] * sign,
            },
            viewport,
        )
    };
    let [a, b, c, d] = [
        edge(start, 1.0),
        edge(end, 1.0),
        edge(end, -1.0),
        edge(start, -1.0),
    ];
    if [a, b, c, d]
        .into_iter()
        .flatten()
        .any(|value| !value.is_finite())
    {
        return;
    }
    // Exactly two existing procedural triangle packets per continuous CSS-pixel
    // strip: at most 512 instances, no atlas lookup or new packet allocation.
    for [first, second, third] in [[a, b, c], [a, c, d]] {
        sprites.push(Sprite {
            position: first,
            radius: second,
            color: [third[0], third[1], 0.0, -2.0],
            uv: GRID_COLOR,
            depths: [f32::INFINITY; 4],
            terrain_blend: [[0.0; 4]; 2],
            pages: [0; 4],
        });
    }
}

fn to_clip(point: ScreenPoint, viewport: [f64; 2]) -> [f32; 2] {
    [
        (point.x / viewport[0].max(1.0) * 2.0 - 1.0) as f32,
        (1.0 - point.y / viewport[1].max(1.0) * 2.0) as f32,
    ]
}

fn pixel_radius(viewport: [f64; 2], pixels: f64) -> [f32; 2] {
    [
        (pixels / viewport[0].max(1.0)) as f32,
        (pixels / viewport[1].max(1.0)) as f32,
    ]
}

fn solid_uv() -> [f32; 4] {
    [
        0.0,
        0.0,
        1.0 / GAME_ATLAS_SIDE as f32,
        1.0 / GAME_ATLAS_SIDE as f32,
    ]
}

// Pinned Rust 1.93.1 wasm32 sin/cos at the fixed ring angles.
const RING_POINTS: [[f64; 2]; 32] = [
    [1.0, 0.0],
    [0.9807852804032304, 0.19509032201612825],
    [0.9238795325112867, 0.3826834323650898],
    [0.8314696123025452, 0.5555702330196022],
    [0.7071067811865476, 0.7071067811865475],
    [0.5555702330196023, 0.8314696123025452],
    [0.38268343236508984, 0.9238795325112867],
    [0.19509032201612833, 0.9807852804032304],
    [6.123233995736766e-17, 1.0],
    [-0.1950903220161282, 0.9807852804032304],
    [-0.3826834323650897, 0.9238795325112867],
    [-0.555570233019602, 0.8314696123025453],
    [-0.7071067811865475, 0.7071067811865476],
    [-0.8314696123025453, 0.5555702330196022],
    [-0.9238795325112867, 0.3826834323650899],
    [-0.9807852804032304, 0.1950903220161286],
    [-1.0, 1.2246467991473532e-16],
    [-0.9807852804032304, -0.19509032201612836],
    [-0.9238795325112868, -0.38268343236508967],
    [-0.8314696123025455, -0.555570233019602],
    [-0.7071067811865477, -0.7071067811865475],
    [-0.5555702330196022, -0.8314696123025452],
    [-0.38268343236509034, -0.9238795325112865],
    [-0.19509032201612866, -0.9807852804032303],
    [-1.8369701987210297e-16, -1.0],
    [0.1950903220161283, -0.9807852804032304],
    [0.38268343236509, -0.9238795325112866],
    [0.5555702330196018, -0.8314696123025455],
    [0.7071067811865474, -0.7071067811865477],
    [0.8314696123025452, -0.5555702330196022],
    [0.9238795325112865, -0.3826834323650904],
    [0.9807852804032303, -0.19509032201612872],
];

#[path = "game_grid/tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
#[wasm_bindgen_test::wasm_bindgen_test]
fn fixed_ring_points_match_original_trigonometry_bits() {
    for (index, [cos, sin]) in RING_POINTS.into_iter().enumerate() {
        let angle = index as f64 * std::f64::consts::TAU / 32.0;
        assert_eq!(cos.to_bits(), angle.cos().to_bits());
        assert_eq!(sin.to_bits(), angle.sin().to_bits());
    }
}
