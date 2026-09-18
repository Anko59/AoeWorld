use crate::{GameArt, GameFrame, SceneCamera, web::Sprite};
use aoe_core::{Camera, ScreenPoint, TileCoord};

const MAX_VISIBLE_TERRAIN_SPRITES: usize = 4_096;

/// Builds a bounded layer of local terrain sprites beneath world entities.
/// Source-specific tile materials join this layer when terrain pages are
/// streamed to the renderer.
pub(crate) fn visible_grass_frames(art: &GameArt, camera: SceneCamera) -> Vec<(Sprite, GameFrame)> {
    if art.grass.is_empty() {
        return Vec::new();
    }
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
    };
    let bounds = visible_bounds(projection);
    let width = usize::try_from(bounds.1.x.saturating_sub(bounds.0.x)).unwrap_or(0);
    let height = usize::try_from(bounds.1.y.saturating_sub(bounds.0.y)).unwrap_or(0);
    let stride = ((width.saturating_mul(height)).div_ceil(MAX_VISIBLE_TERRAIN_SPRITES) as f64)
        .sqrt()
        .ceil()
        .max(1.0) as i32;
    let capacity = (width / stride as usize + 1) * (height / stride as usize + 1);
    let mut result = Vec::with_capacity(capacity);
    for y in (bounds.0.y..bounds.1.y).step_by(stride as usize) {
        for x in (bounds.0.x..bounds.1.x).step_by(stride as usize) {
            let frame = art.grass[((x * 7 + y * 13).unsigned_abs() as usize) % art.grass.len()];
            let screen = projection.world_to_screen([f64::from(x), f64::from(y)]);
            let width = f64::from(frame.size[0]) * camera.zoom;
            let height = f64::from(frame.size[1]) * camera.zoom;
            if screen.x + width < 0.0
                || screen.y + height < 0.0
                || screen.x - width > camera.viewport[0]
                || screen.y - height > camera.viewport[1]
            {
                continue;
            }
            let x = screen.x - f64::from(frame.anchor[0]) * camera.zoom;
            let y = screen.y - f64::from(frame.anchor[1]) * camera.zoom;
            result.push((
                Sprite {
                    position: [
                        ((x + width / 2.0) / camera.viewport[0] * 2.0 - 1.0) as f32,
                        (1.0 - (y + height / 2.0) / camera.viewport[1] * 2.0) as f32,
                    ],
                    radius: [
                        (width / camera.viewport[0]) as f32,
                        (height / camera.viewport[1]) as f32,
                    ],
                    color: [1.0; 4],
                    uv: frame.uv,
                },
                scaled(frame, camera.zoom as f32),
            ));
        }
    }
    result
}

fn visible_bounds(camera: Camera) -> (TileCoord, TileCoord) {
    let corners = [
        camera.screen_to_world(ScreenPoint { x: 0.0, y: 0.0 }),
        camera.screen_to_world(ScreenPoint {
            x: camera.viewport[0],
            y: 0.0,
        }),
        camera.screen_to_world(ScreenPoint {
            x: 0.0,
            y: camera.viewport[1],
        }),
        camera.screen_to_world(ScreenPoint {
            x: camera.viewport[0],
            y: camera.viewport[1],
        }),
    ];
    let min_x = corners
        .iter()
        .map(|point| point[0])
        .fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|point| point[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners
        .iter()
        .map(|point| point[1])
        .fold(f64::INFINITY, f64::min);
    let max_y = corners
        .iter()
        .map(|point| point[1])
        .fold(f64::NEG_INFINITY, f64::max);
    (
        TileCoord::new(min_x.floor() as i32 - 1, min_y.floor() as i32 - 1),
        TileCoord::new(max_x.ceil() as i32 + 2, max_y.ceil() as i32 + 2),
    )
}

fn scaled(mut frame: GameFrame, scale: f32) -> GameFrame {
    frame.size = frame.size.map(|value| value * scale);
    frame.anchor = frame.anchor.map(|value| value * scale);
    frame
}
