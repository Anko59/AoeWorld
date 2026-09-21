use crate::{GameArt, GameFrame, SceneCamera, SceneTerrain, web::Sprite};
use aoe_core::{Camera, ScreenPoint, TileCoord};
use std::collections::BTreeMap;

const MAX_VISIBLE_TERRAIN_SPRITES: usize = 4_096;

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

fn visible_map_terrain_frames(
    art: &GameArt,
    terrain: &[SceneTerrain],
    camera: SceneCamera,
) -> Vec<(Sprite, GameFrame)> {
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
    };
    let mut visible = Vec::with_capacity(terrain.len().min(MAX_VISIBLE_TERRAIN_SPRITES));
    for sample in terrain {
        let frames = art
            .terrain
            .get(usize::from(sample.material))
            .filter(|frames| !frames.is_empty())
            .unwrap_or(&art.grass);
        let frame = frames[((sample.position[0] as i32 * 7 + sample.position[1] as i32 * 13)
            .unsigned_abs() as usize)
            % frames.len()];
        if let Some(candidate) = terrain_candidate(&projection, *sample, frame, camera, 1) {
            visible.push(candidate);
        }
    }
    if visible.len() <= MAX_VISIBLE_TERRAIN_SPRITES {
        return visible
            .into_iter()
            .filter_map(|candidate| terrain_sprite(&projection, candidate, camera))
            .collect();
    }

    let (cell_size, cells) = aggregate_candidates(visible);
    cells
        .into_iter()
        .filter_map(|((cell_x, cell_y), candidate)| {
            let center = [
                f64::from(cell_x * cell_size) + f64::from(cell_size) * 0.5,
                f64::from(cell_y * cell_size) + f64::from(cell_size) * 0.5,
            ];
            terrain_sprite(
                &projection,
                TerrainCandidate {
                    sample: SceneTerrain {
                        position: center,
                        ..candidate.sample
                    },
                    frame: candidate.frame,
                    scale: candidate.scale * cell_size as f32,
                },
                camera,
            )
        })
        .collect()
}

fn aggregate_candidates(
    visible: Vec<TerrainCandidate>,
) -> (i32, BTreeMap<(i32, i32), TerrainCandidate>) {
    let mut cell_size = (visible.len().div_ceil(MAX_VISIBLE_TERRAIN_SPRITES) as f64)
        .sqrt()
        .ceil() as i32;
    loop {
        let mut cells = BTreeMap::new();
        for candidate in visible.iter().copied() {
            let cell = (
                (candidate.sample.position[0].floor() as i32).div_euclid(cell_size),
                (candidate.sample.position[1].floor() as i32).div_euclid(cell_size),
            );
            cells.entry(cell).or_insert(candidate);
        }
        if cells.len() <= MAX_VISIBLE_TERRAIN_SPRITES {
            return (cell_size, cells);
        }
        cell_size = cell_size.saturating_add(1);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    wasm_bindgen_test_configure!(run_in_browser);

    fn test_art(frame: GameFrame) -> GameArt {
        GameArt {
            walking: Vec::new(),
            standing: Vec::new(),
            grass: vec![frame],
            terrain: std::array::from_fn(|_| vec![frame]),
            resources: std::array::from_fn(|_| Vec::new()),
        }
    }

    #[wasm_bindgen_test]
    fn sparse_terrain_aggregation_stays_within_the_sprite_bound() {
        let frame = GameFrame {
            uv: [0.0; 4],
            size: [1.0, 1.0],
            anchor: [0.0, 0.0],
        };
        let candidates = (0..8_192)
            .map(|x| TerrainCandidate {
                sample: SceneTerrain {
                    position: [f64::from(x * 2), 0.0],
                    material: 0,
                    elevation_meters: 0.0,
                },
                frame,
                scale: 1.0,
            })
            .collect();
        let (cell_size, cells) = aggregate_candidates(candidates);
        assert_eq!(cell_size, 4);
        assert_eq!(cells.len(), MAX_VISIBLE_TERRAIN_SPRITES);
        assert!(cells.contains_key(&(0, 0)));
        assert!(cells.contains_key(&(4_095, 0)));
    }

    #[wasm_bindgen_test]
    fn offscreen_cached_tiles_do_not_change_visible_terrain() {
        let frame = GameFrame {
            uv: [0.0; 4],
            size: [1.0, 1.0],
            anchor: [0.0, 0.0],
        };
        let art = test_art(frame);
        let camera = SceneCamera {
            center: [0.0, 0.0],
            zoom: 1.0,
            viewport: [256.0, 128.0],
        };
        let visible = [SceneTerrain {
            position: [0.0, 0.0],
            material: 0,
            elevation_meters: 0.0,
        }];
        let mut populated = visible.to_vec();
        populated.extend((0..10_000).map(|x| SceneTerrain {
            position: [10_000.0 + f64::from(x), 0.0],
            material: 0,
            elevation_meters: 0.0,
        }));
        let baseline = visible_terrain_frames(&art, &visible, camera);
        let cached = visible_terrain_frames(&art, &populated, camera);
        assert_eq!(baseline.len(), 1);
        assert_eq!(cached.len(), baseline.len());
        assert_eq!(cached[0].0.position, baseline[0].0.position);
    }
}
