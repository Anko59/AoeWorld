use crate::{GAME_ATLAS_SIDE, SceneCamera, surface_mesh::surface_depth, web::Sprite};
use aoe_core::{Camera, ScreenPoint, WorldConfig};
use web_sys::CanvasRenderingContext2d;

const GRID_COLOR: [f32; 4] = [0.75, 0.9, 0.6, 0.22];
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
            },
            surface_depth([world[0], world[1], elevation_meters]),
        ));
    }
    sprites
}

pub(crate) fn grid_sprites(camera: SceneCamera) -> Vec<Sprite> {
    let lines = grid_lines(camera, WorldConfig::default());
    let mut sprites = Vec::new();
    for (start, end) in lines {
        add_line(&mut sprites, start, end, camera.viewport);
    }
    sprites
}

pub(crate) fn draw_grid(context: &CanvasRenderingContext2d, camera: SceneCamera) {
    context.begin_path();
    context.set_stroke_style_str("rgba(220,235,170,.22)");
    for (start, end) in grid_lines(camera, WorldConfig::default()) {
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

fn grid_lines(camera: SceneCamera, config: WorldConfig) -> Vec<(ScreenPoint, ScreenPoint)> {
    let projection = camera_projection(camera);
    let visible = projection.visible_tiles(config, 1.0);
    let min_x = visible.min.x.saturating_sub(1);
    let max_x = visible.max.x.saturating_add(1);
    let min_y = visible.min.y.saturating_sub(1);
    let max_y = visible.max.y.saturating_add(1);
    let mut lines = Vec::new();
    for x in min_x..=max_x {
        add_clipped_line(
            &mut lines,
            projection.world_to_screen([f64::from(x), f64::from(min_y)]),
            projection.world_to_screen([f64::from(x), f64::from(max_y)]),
            camera.viewport,
        );
    }
    for y in min_y..=max_y {
        add_clipped_line(
            &mut lines,
            projection.world_to_screen([f64::from(min_x), f64::from(y)]),
            projection.world_to_screen([f64::from(max_x), f64::from(y)]),
            camera.viewport,
        );
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
    let dx = end.x - start.x;
    let dy = end.y - start.y;
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
    let distance = ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt();
    let steps = (distance / 8.0).ceil().max(1.0) as usize;
    for step in 0..=steps {
        let amount = f64::from(step as u32) / f64::from(steps as u32);
        let point = ScreenPoint {
            x: start.x + (end.x - start.x) * amount,
            y: start.y + (end.y - start.y) * amount,
        };
        sprites.push(Sprite {
            position: to_clip(point, viewport),
            radius: pixel_radius(viewport, 2.0),
            color: GRID_COLOR,
            uv: solid_uv(),
            depths: [f32::INFINITY; 4],
            terrain_blend: [[0.0; 4]; 2],
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

#[cfg(test)]
#[wasm_bindgen_test::wasm_bindgen_test]
fn fixed_ring_points_match_original_trigonometry_bits() {
    for (index, [cos, sin]) in RING_POINTS.into_iter().enumerate() {
        let angle = index as f64 * std::f64::consts::TAU / 32.0;
        assert_eq!(cos.to_bits(), angle.cos().to_bits());
        assert_eq!(sin.to_bits(), angle.sin().to_bits());
    }
}
