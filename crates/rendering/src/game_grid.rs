use crate::{GAME_ATLAS_SIDE, SceneCamera, web::Sprite};
use aoe_core::{Camera, ScreenPoint, WorldConfig};
use web_sys::CanvasRenderingContext2d;

const GRID_COLOR: [f32; 4] = [0.75, 0.9, 0.6, 0.22];

pub(crate) fn selection_ring(camera: SceneCamera, position: [f64; 2]) -> Vec<Sprite> {
    let screen = camera_projection(camera).world_to_screen(position);
    let radius_x = 24.0 * camera.zoom;
    let radius_y = 8.0 * camera.zoom;
    let mut sprites = Vec::with_capacity(32);
    for index in 0..32 {
        let angle = f64::from(index) * std::f64::consts::TAU / 32.0;
        sprites.push(Sprite {
            position: to_clip(
                ScreenPoint {
                    x: screen.x + angle.cos() * radius_x,
                    y: screen.y + angle.sin() * radius_y,
                },
                camera.viewport,
            ),
            radius: pixel_radius(camera.viewport, 1.5),
            color: [0.95, 0.85, 0.35, 1.0],
            uv: solid_uv(),
        });
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
