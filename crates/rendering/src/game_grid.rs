use crate::{
    SceneCamera,
    surface_mesh::{ProjectedSurfaceTriangle, surface_depth},
    web::Sprite,
};
use aoe_core::{Camera, ISO_TILE_HEIGHT, ISO_TILE_WIDTH, ScreenPoint};
use web_sys::CanvasRenderingContext2d;

/// Straight-alpha overlay colour; `draw_grid` spells the same value for Canvas.
const GRID_COLOR: [f32; 4] = [0.05, 0.08, 0.03, 0.6];
/// Screen pixels kept between neighbouring parallel lines.
const GRID_MIN_SPACING: f64 = 16.0;
pub(crate) const SELECTION_RING_SPRITES: usize = 1;

pub(crate) fn selection_ring(
    camera: SceneCamera,
    position: [f64; 2],
    elevation_meters: f64,
) -> Vec<(Sprite, f64)> {
    selection_ellipse(camera, position, elevation_meters, [24.0, 8.0])
}

pub(crate) fn selection_ellipse(
    camera: SceneCamera,
    position: [f64; 2],
    elevation_meters: f64,
    radius: [f64; 2],
) -> Vec<(Sprite, f64)> {
    let screen = camera_projection(camera).world_to_screen_at_height(position, elevation_meters);
    // One analytic ellipse, with a constant two-pixel stroke. It clears its
    // ground surface but stays below the unit body in world depth order.
    let radii = [
        (radius[0] * camera.zoom).max(5.0),
        (radius[1] * camera.zoom).max(3.0),
    ];
    // Flat ground at this elevation is nearer the viewer by `slope` depth units
    // per pixel below the centre, so the stroke's depth must grow downward with
    // it or the ground covers the front arc. Renderers read `depths[0]` for the
    // top edge and `depths[1]` for the bottom edge, as offsets from the layer
    // depth, and interpolate between them.
    let below = camera_projection(camera)
        .world_to_screen_at_height([position[0] + 1.0, position[1] + 1.0], elevation_meters);
    let slope = if (below.y - screen.y).abs() > f64::EPSILON {
        2.0 / (below.y - screen.y)
    } else {
        0.0
    };
    let half = ((radii[1] + 1.0) * slope) as f32;
    vec![(
        Sprite {
            position: to_clip(screen, camera.viewport),
            radius: [
                (2.0 * (radii[0] + 1.0) / camera.viewport[0]) as f32,
                (2.0 * (radii[1] + 1.0) / camera.viewport[1]) as f32,
            ],
            color: [0.95, 0.85, 0.35, 1.0],
            uv: [radii[0] as f32, radii[1] as f32, 2.0, 0.0],
            depths: [-half, half, half, -half],
            terrain_blend: [[0.0; 4]; 2],
            pages: [2, 0, 0, 2],
        },
        surface_depth([position[0], position[1], elevation_meters]) + 1.4,
    )]
}

/// GPU instance depths for a selection ellipse: the stroke's top and bottom
/// edge depths relative to the camera origin, kept in the first three slots so
/// depth normalization sees both.
pub(crate) fn selection_instance_depths(
    sprite: &Sprite,
    layer_depth: f64,
    origin: f64,
) -> [f32; 4] {
    let top = (layer_depth - origin) as f32 + sprite.depths[0];
    let bottom = (layer_depth - origin) as f32 + sprite.depths[1];
    [top, bottom, bottom, top]
}

/// Canvas fragment depth of a selection ellipse at a pixel row.
pub(crate) fn selection_depth_at(
    sprite: &Sprite,
    layer_depth: f64,
    row: f64,
    viewport_height: f64,
) -> f64 {
    let center = (1.0 - f64::from(sprite.position[1])) * viewport_height * 0.5;
    let half_height = f64::from(sprite.radius[1]) * viewport_height * 0.5;
    let t = ((row - center) / half_height).clamp(-1.0, 1.0);
    let (top, bottom) = (f64::from(sprite.depths[0]), f64::from(sprite.depths[1]));
    layer_depth + top + (bottom - top) * (t + 1.0) * 0.5
}

/// Borders of the drawn terrain cells, so the overlay follows the mesh's
/// heights and level of detail. Every border comes from the cell on its
/// positive side; lines closer than `GRID_MIN_SPACING` thin out by twos.
pub(crate) fn grid_segments<'a>(
    surfaces: impl IntoIterator<Item = &'a ProjectedSurfaceTriangle>,
    zoom: f64,
) -> Vec<[ScreenPoint; 2]> {
    let mut step = 1.0;
    // Neighbouring parallel lines of one family sit `step` world tiles apart,
    // a perpendicular screen distance of W * H / sqrt(W^2 + H^2) per tile in
    // the 2:1 projection — not the tile height, which overstates it.
    let per_tile = ISO_TILE_WIDTH * ISO_TILE_HEIGHT
        / (ISO_TILE_WIDTH * ISO_TILE_WIDTH + ISO_TILE_HEIGHT * ISO_TILE_HEIGHT).sqrt();
    while step * per_tile * zoom < GRID_MIN_SPACING && step < 1e9 {
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

#[path = "game_grid/tests/ellipse.rs"]
#[cfg(test)]
mod tests;
