use crate::{GameArt, GameFrame, SceneCamera, SceneTerrain, web::Sprite};
use aoe_core::{Camera, ISO_TILE_HEIGHT, ISO_TILE_WIDTH, ScreenPoint, TileCoord};
use std::collections::BTreeMap;
const MAX_VISIBLE_TERRAIN_SPRITES: usize = 4_096;
const TERRAIN_NATIVE_DIAMOND_WIDTH: f32 = 96.0;
const TERRAIN_NATIVE_DIAMOND_HEIGHT: f32 = 48.0;

/// Builds a bounded layer of local terrain sprites beneath world entities.
/// A map supplies semantic material groups; diagnostic worlds retain the
/// existing grass fallback before any immutable chunks arrive.
pub(crate) fn visible_terrain_frames(
    art: &GameArt,
    terrain: &[SceneTerrain],
    camera: SceneCamera,
) -> Vec<(Sprite, GameFrame)> {
    if art.grass.is_empty() {
        return Vec::new();
    }
    if !terrain.is_empty() {
        return visible_map_terrain_frames(art, terrain, camera);
    }
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let bounds = visible_bounds(projection);
    let mut cell_size = 1_i32;
    while canonical_cell_count(bounds, cell_size) > MAX_VISIBLE_TERRAIN_SPRITES {
        cell_size = cell_size.saturating_add(1);
    }
    let ((min_cell_x, max_cell_x), (min_cell_y, max_cell_y)) =
        canonical_cell_bounds(bounds, cell_size);
    let mut cells = BTreeMap::new();
    for cell_y in min_cell_y..max_cell_y {
        for cell_x in min_cell_x..max_cell_x {
            let position = cell_center((cell_x, cell_y), cell_size);
            let frame = terrain_frame(
                art.grass[((cell_x.wrapping_mul(7) + cell_y.wrapping_mul(13)).unsigned_abs()
                    as usize)
                    % art.grass.len()],
            );
            if let Some(candidate) = terrain_candidate(
                &projection,
                SceneTerrain {
                    position,
                    material: 0,
                    elevation_meters: 0.0,
                },
                frame,
                camera,
                cell_size,
            ) {
                cells.insert((cell_x, cell_y), candidate);
            }
        }
    }
    render_cells(&projection, camera, cell_size, cells)
}

fn visible_map_terrain_frames(
    art: &GameArt,
    terrain: &[SceneTerrain],
    camera: SceneCamera,
) -> Vec<(Sprite, GameFrame)> {
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let cell_size = map_cell_size(art, terrain, camera, &projection);
    let mut cells = BTreeMap::new();
    for sample in terrain {
        if let Some(candidate) = map_candidate(art, *sample, camera, &projection) {
            let cell = cell_key(candidate.sample.position, cell_size);
            cells.entry(cell).or_insert(candidate);
        }
    }
    render_cells(&projection, camera, cell_size, cells)
}

fn map_cell_size(
    art: &GameArt,
    terrain: &[SceneTerrain],
    camera: SceneCamera,
    projection: &Camera,
) -> i32 {
    let mut cell_size = 1;
    loop {
        let mut cells = BTreeMap::new();
        for sample in terrain {
            if let Some(candidate) = map_candidate(art, *sample, camera, projection) {
                cells.insert(cell_key(candidate.sample.position, cell_size), ());
                if cells.len() > MAX_VISIBLE_TERRAIN_SPRITES {
                    break;
                }
            }
        }
        if cells.len() <= MAX_VISIBLE_TERRAIN_SPRITES {
            return cell_size;
        }
        cell_size = cell_size.saturating_add(1);
    }
}

fn map_candidate(
    art: &GameArt,
    sample: SceneTerrain,
    camera: SceneCamera,
    projection: &Camera,
) -> Option<TerrainCandidate> {
    let frames = art
        .terrain
        .get(usize::from(sample.material))
        .filter(|frames| !frames.is_empty())
        .unwrap_or(&art.grass);
    let frame = terrain_frame(
        frames[((sample.position[0] as i32 * 7 + sample.position[1] as i32 * 13).unsigned_abs()
            as usize)
            % frames.len()],
    );
    terrain_candidate(projection, sample, frame, camera, 1)
}

fn cell_key(position: [f64; 2], cell_size: i32) -> (i32, i32) {
    (
        (position[0].floor() as i32).div_euclid(cell_size),
        (position[1].floor() as i32).div_euclid(cell_size),
    )
}

fn canonical_cell_bounds(
    bounds: (TileCoord, TileCoord),
    cell_size: i32,
) -> ((i32, i32), (i32, i32)) {
    (
        (
            bounds.0.x.div_euclid(cell_size),
            ceil_div(bounds.1.x, cell_size),
        ),
        (
            bounds.0.y.div_euclid(cell_size),
            ceil_div(bounds.1.y, cell_size),
        ),
    )
}

fn ceil_div(value: i32, divisor: i32) -> i32 {
    let quotient = value.div_euclid(divisor);
    if value.rem_euclid(divisor) == 0 {
        quotient
    } else {
        quotient.saturating_add(1)
    }
}

fn canonical_cell_count(bounds: (TileCoord, TileCoord), cell_size: i32) -> usize {
    let ((min_x, max_x), (min_y, max_y)) = canonical_cell_bounds(bounds, cell_size);
    let width = max_x.saturating_sub(min_x).max(0) as usize;
    let height = max_y.saturating_sub(min_y).max(0) as usize;
    width.saturating_mul(height)
}

fn cell_center((cell_x, cell_y): (i32, i32), cell_size: i32) -> [f64; 2] {
    let cell_size = f64::from(cell_size);
    [
        f64::from(cell_x) * cell_size + cell_size * 0.5,
        f64::from(cell_y) * cell_size + cell_size * 0.5,
    ]
}

fn render_cells(
    projection: &Camera,
    camera: SceneCamera,
    cell_size: i32,
    cells: BTreeMap<(i32, i32), TerrainCandidate>,
) -> Vec<(Sprite, GameFrame)> {
    cells
        .into_iter()
        .filter_map(|((cell_x, cell_y), candidate)| {
            let center = cell_center((cell_x, cell_y), cell_size);
            terrain_sprite(
                projection,
                TerrainCandidate {
                    sample: SceneTerrain {
                        position: center,
                        ..candidate.sample
                    },
                    frame: candidate.frame,
                    scale: cell_size as f32,
                },
                camera,
            )
        })
        .collect()
}

#[derive(Clone, Copy)]
struct TerrainCandidate {
    sample: SceneTerrain,
    frame: GameFrame,
    scale: f32,
}

fn terrain_candidate(
    projection: &Camera,
    sample: SceneTerrain,
    frame: GameFrame,
    camera: SceneCamera,
    scale: i32,
) -> Option<TerrainCandidate> {
    let screen = projection.world_to_screen_at_height(sample.position, sample.elevation_meters);
    let width = f64::from(frame.size[0]) * camera.zoom * f64::from(scale);
    let height = f64::from(frame.size[1]) * camera.zoom * f64::from(scale);
    (screen.x + width >= 0.0
        && screen.y + height >= 0.0
        && screen.x - width <= camera.viewport[0]
        && screen.y - height <= camera.viewport[1])
        .then_some(TerrainCandidate {
            sample,
            frame,
            scale: scale as f32,
        })
}

fn terrain_sprite(
    projection: &Camera,
    candidate: TerrainCandidate,
    camera: SceneCamera,
) -> Option<(Sprite, GameFrame)> {
    let TerrainCandidate {
        sample,
        frame,
        scale,
    } = candidate;
    let screen = projection.world_to_screen_at_height(sample.position, sample.elevation_meters);
    let frame = scaled(frame, camera.zoom as f32 * scale);
    let width = f64::from(frame.size[0]);
    let height = f64::from(frame.size[1]);
    if screen.x + width < 0.0
        || screen.y + height < 0.0
        || screen.x - width > camera.viewport[0]
        || screen.y - height > camera.viewport[1]
    {
        return None;
    }
    let x = screen.x - f64::from(frame.anchor[0]);
    let y = screen.y - f64::from(frame.anchor[1]);
    Some((
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
        frame,
    ))
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

fn terrain_frame(mut frame: GameFrame) -> GameFrame {
    let scale_x = ISO_TILE_WIDTH as f32 / TERRAIN_NATIVE_DIAMOND_WIDTH;
    let scale_y = ISO_TILE_HEIGHT as f32 / TERRAIN_NATIVE_DIAMOND_HEIGHT;
    frame.size[0] *= scale_x;
    frame.size[1] *= scale_y;
    frame.anchor = [
        TERRAIN_NATIVE_DIAMOND_WIDTH * 0.5 * scale_x,
        TERRAIN_NATIVE_DIAMOND_HEIGHT * 0.5 * scale_y,
    ];
    frame
}

#[cfg(test)]
#[path = "terrain_tests.rs"]
mod tests;
