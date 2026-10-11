use crate::{
    GAME_ATLAS_SIDE, SceneCamera,
    surface_mesh::{ProjectedSurfaceTriangle, surface_depth},
    web::Sprite,
};
use aoe_core::{Camera, ISO_TILE_HEIGHT, ScreenPoint};
use web_sys::CanvasRenderingContext2d;

/// Straight-alpha overlay colour; `draw_grid` spells the same value for Canvas.
const GRID_COLOR: [f32; 4] = [0.05, 0.08, 0.03, 0.6];
/// Screen pixels kept between neighbouring parallel lines.
const GRID_MIN_SPACING: f64 = 16.0;
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

/// Borders of the drawn terrain cells, so the overlay follows the mesh's
/// heights and level of detail. Every border comes from the cell on its
/// positive side; lines closer than `GRID_MIN_SPACING` thin out by twos.
pub(crate) fn grid_segments<'a>(
    surfaces: impl IntoIterator<Item = &'a ProjectedSurfaceTriangle>,
    zoom: f64,
) -> Vec<[ScreenPoint; 2]> {
    let mut step = 1.0;
    while step * ISO_TILE_HEIGHT * zoom < GRID_MIN_SPACING && step < 1e9 {
        step *= 2.0;
    }
    let mut segments = Vec::new();
    for triangle in surfaces.into_iter().filter(|triangle| !triangle.skirt) {
        let points = triangle.points;
        for index in 0..3 {
            let [start, end, apex] = [
                points[index],
                points[(index + 1) % 3],
                points[(index + 2) % 3],
            ];
            for axis in 0..2 {
                let line = start.world[axis];
                if line == end.world[axis]
                    && apex.world[axis] > line
                    && line.rem_euclid(step) == 0.0
                {
                    segments.push([start.screen, end.screen]);
                }
            }
        }
    }
    segments
}

/// One-pixel lines as two solid, alpha-blended surface triangles each.
pub(crate) fn grid_sprites(segments: &[[ScreenPoint; 2]], viewport: [f64; 2]) -> Vec<Sprite> {
    let mut sprites = Vec::with_capacity(segments.len() * 2);
    for [start, end] in segments {
        let corner = |point: &ScreenPoint, offset: f64| {
            to_clip(
                ScreenPoint {
                    x: point.x,
                    y: point.y + offset,
                },
                viewport,
            )
        };
        let far = corner(end, 0.5);
        for [second, third] in [[corner(start, 0.5), far], [far, corner(end, -0.5)]] {
            sprites.push(Sprite {
                position: corner(start, -0.5),
                radius: second,
                color: [third[0], third[1], 0.0, -2.0],
                uv: GRID_COLOR,
                depths: [f32::INFINITY; 4],
                terrain_blend: [[0.0; 4]; 2],
                pages: [0; 4],
            });
        }
    }
    sprites
}

pub(crate) fn draw_grid(context: &CanvasRenderingContext2d, segments: &[[ScreenPoint; 2]]) {
    context.begin_path();
    context.set_stroke_style_str("rgba(13,20,8,.6)");
    for [start, end] in segments {
        context.move_to(start.x, start.y);
        context.line_to(end.x, end.y);
    }
    context.stroke();
}

fn camera_projection(camera: SceneCamera) -> Camera {
    Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
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

#[cfg(test)]
#[wasm_bindgen_test::wasm_bindgen_test]
fn fixed_ring_points_match_original_trigonometry_bits() {
    for (index, [cos, sin]) in RING_POINTS.into_iter().enumerate() {
        let angle = index as f64 * std::f64::consts::TAU / 32.0;
        assert_eq!(cos.to_bits(), angle.cos().to_bits());
        assert_eq!(sin.to_bits(), angle.sin().to_bits());
    }
}
