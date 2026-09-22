use super::*;
use crate::surface_mesh::{
    MAX_SURFACE_TILES, barycentric, pick_surface_point, projected_surface_triangles,
};
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

fn sprites_cover_viewport(frames: &[(Sprite, GameFrame)], viewport: [f64; 2]) -> bool {
    for y in (32..(viewport[1] as usize - 32)).step_by(32) {
        for x in (32..(viewport[0] as usize - 32)).step_by(32) {
            let x = x as f64;
            let y = y as f64;
            let covered = frames.iter().any(|(sprite, frame)| {
                let frame_center_x = (f64::from(sprite.position[0]) + 1.0) * viewport[0] * 0.5;
                let frame_center_y = (1.0 - f64::from(sprite.position[1])) * viewport[1] * 0.5;
                let center_x =
                    frame_center_x + f64::from(frame.anchor[0]) - f64::from(frame.size[0]) * 0.5;
                let center_y =
                    frame_center_y + f64::from(frame.anchor[1]) - f64::from(frame.size[1]) * 0.5;
                let half_width = f64::from(frame.anchor[0]).max(1.0);
                let half_height = f64::from(frame.anchor[1]).max(1.0);
                let inclusive_edge = 1.0 / half_width.min(half_height);
                let diamond_distance =
                    (x - center_x).abs() / half_width + (y - center_y).abs() / half_height;
                diamond_distance <= 1.0 + inclusive_edge
            });
            if !covered {
                return false;
            }
        }
    }
    true
}

#[wasm_bindgen_test]
fn sparse_terrain_aggregation_stays_within_the_sprite_bound() {
    let frame = GameFrame {
        uv: [0.0; 4],
        size: [97.0, 49.0],
        anchor: [0.0, 0.0],
    };
    let art = test_art(frame);
    let terrain = (0..8_192)
        .map(|x| SceneTerrain {
            position: [f64::from(x * 2), 0.0],
            material: 0,
            elevation_meters: 0.0,
            surface: SceneTerrainSurface::flat(0.0),
        })
        .collect::<Vec<_>>();
    let camera = SceneCamera {
        center: [8_191.0, 0.0],
        zoom: 1.0,
        viewport: [2_000_000.0, 2_000_000.0],
        focus_elevation_meters: 0.0,
    };
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let cell_size = map_cell_size(&art, &terrain, camera, &projection);
    let mut cells = BTreeMap::new();
    for sample in &terrain {
        if let Some(candidate) = map_candidate(&art, *sample, camera, &projection) {
            cells.insert(cell_key(candidate.sample.position, cell_size), candidate);
        }
    }
    assert_eq!(cell_size, 4);
    assert_eq!(cells.len(), MAX_VISIBLE_TERRAIN_SPRITES);
    assert!(cells.contains_key(&(0, 0)));
    assert!(cells.contains_key(&(4_095, 0)));
}

#[wasm_bindgen_test]
fn terrain_frames_normalize_the_reviewed_native_diamond_and_anchor() {
    let frame = terrain_frame(GameFrame {
        uv: [0.0; 4],
        size: [97.0, 49.0],
        anchor: [0.0, 0.0],
    });
    assert!((frame.size[0] - 97.0 * 4.0 / 3.0).abs() < 1e-4);
    assert!((frame.size[1] - 49.0 * 4.0 / 3.0).abs() < 1e-4);
    assert!((frame.anchor[0] - 64.0).abs() < 1e-4);
    assert!((frame.anchor[1] - 32.0).abs() < 1e-4);
}

#[wasm_bindgen_test]
fn nonzero_height_terrain_is_culled_relative_to_camera_focus() {
    let art = test_art(GameFrame {
        uv: [0.0; 4],
        size: [97.0, 49.0],
        anchor: [0.0, 0.0],
    });
    let camera = SceneCamera {
        center: [0.0, 0.0],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 6.0,
    };
    let elevated = [SceneTerrain {
        position: [0.0, 0.0],
        material: 0,
        elevation_meters: 6.0,
        surface: SceneTerrainSurface::flat(6.0),
    }];
    let ground = [SceneTerrain {
        position: [0.0, 0.0],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
    }];
    assert_eq!(visible_terrain_frames(&art, &elevated, camera).len(), 1);
    assert!(visible_terrain_frames(&art, &ground, camera).is_empty());
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
        focus_elevation_meters: 0.0,
    };
    let visible = [SceneTerrain {
        position: [0.0, 0.0],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
    }];
    let mut populated = visible.to_vec();
    populated.extend((0..10_000).map(|x| SceneTerrain {
        position: [10_000.0 + f64::from(x), 0.0],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
    }));
    let baseline = visible_terrain_frames(&art, &visible, camera);
    let cached = visible_terrain_frames(&art, &populated, camera);
    assert_eq!(baseline.len(), 1);
    assert_eq!(cached.len(), baseline.len());
    assert_eq!(cached[0].0.position, baseline[0].0.position);
}

#[wasm_bindgen_test]
fn diagnostic_terrain_uses_world_coordinates_after_camera_translation() {
    let art = test_art(GameFrame {
        uv: [0.0; 4],
        size: [97.0, 49.0],
        anchor: [0.0, 0.0],
    });
    let camera = SceneCamera {
        center: [300.0, 200.0],
        zoom: 0.25,
        viewport: [1_280.0, 720.0],
        focus_elevation_meters: 0.0,
    };
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let bounds = visible_bounds(projection);
    assert!(canonical_cell_count(bounds, 1) > MAX_VISIBLE_TERRAIN_SPRITES);
    let mut cell_size = 1;
    while canonical_cell_count(bounds, cell_size) > MAX_VISIBLE_TERRAIN_SPRITES {
        cell_size += 1;
    }
    assert!(cell_size > 1);
    let frames = visible_terrain_frames(&art, &[], camera);
    assert!(frames.len() <= MAX_VISIBLE_TERRAIN_SPRITES);
    assert!(sprites_cover_viewport(&frames, camera.viewport));
}

#[wasm_bindgen_test]
fn flat_map_terrain_uses_covering_lod_at_translated_camera() {
    let art = test_art(GameFrame {
        uv: [0.0; 4],
        size: [97.0, 49.0],
        anchor: [0.0, 0.0],
    });
    let terrain = (0..256)
        .flat_map(|y| {
            (0..256).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
            })
        })
        .collect::<Vec<_>>();
    let camera = SceneCamera {
        center: [128.0, 128.0],
        zoom: 0.25,
        viewport: [1_920.0, 1_080.0],
        focus_elevation_meters: 0.0,
    };
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let cell_size = map_cell_size(&art, &terrain, camera, &projection);
    assert!(cell_size > 1);
    let frames = visible_terrain_frames(&art, &terrain, camera);
    assert!(frames.len() <= MAX_VISIBLE_TERRAIN_SPRITES);
    assert!(sprites_cover_viewport(&frames, camera.viewport));
}

#[wasm_bindgen_test]
fn projected_surface_uses_corner_heights_and_shared_diagonal() {
    let camera = SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let sample = SceneTerrain {
        position: [0.5, 0.5],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: [0, 0, 1, 1],
            kind: SceneTerrainSurface::RAMP,
            triangulation: 0,
            water: 0,
        },
    };
    let triangles = projected_surface_triangles(&[sample], camera);
    assert_eq!(triangles.len(), 2);
    assert!(!triangles.is_empty());
    let first = &triangles[0];
    assert!(
        first
            .points
            .iter()
            .any(|point| { (point.world[2] - 1.0).abs() < f64::EPSILON })
    );
    let centroid = ScreenPoint {
        x: first.points.iter().map(|point| point.screen.x).sum::<f64>() / 3.0,
        y: first.points.iter().map(|point| point.screen.y).sum::<f64>() / 3.0,
    };
    let Some(picked) = pick_surface_point(&triangles, centroid) else {
        assert!(false, "ramp pick");
        return;
    };
    assert!(picked[0].is_finite() && picked[1].is_finite());
}

#[wasm_bindgen_test]
fn cliff_skirt_requires_a_loaded_neighbor_height_discontinuity() {
    let camera = SceneCamera {
        center: [1.0, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 3.0,
    };
    let cliff = |position: [f64; 2], heights: [i16; 4]| SceneTerrain {
        position,
        material: 4,
        elevation_meters: f64::from(heights[0]),
        surface: SceneTerrainSurface {
            corner_game_height_levels: heights,
            kind: SceneTerrainSurface::CLIFF,
            triangulation: 0,
            water: 0,
        },
    };
    let high = cliff([0.5, 0.5], [3, 3, 3, 3]);
    let equal = cliff([1.5, 0.5], [3, 3, 3, 3]);
    let low = cliff([1.5, 0.5], [1, 1, 1, 1]);
    assert_eq!(projected_surface_triangles(&[high, equal], camera).len(), 4);
    assert_eq!(projected_surface_triangles(&[high, low], camera).len(), 6);
    assert_eq!(projected_surface_triangles(&[high], camera).len(), 2);
}

#[wasm_bindgen_test]
fn south_skirt_compares_shared_corners_in_the_same_order() {
    let camera = SceneCamera {
        center: [0.5, 1.0],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let cliff = |position: [f64; 2], heights: [i16; 4]| SceneTerrain {
        position,
        material: 4,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: heights,
            kind: SceneTerrainSurface::CLIFF,
            triangulation: 0,
            water: 0,
        },
    };
    let north = cliff([0.5, 0.5], [0, 0, 0, 2]);
    let south = cliff([0.5, 1.5], [2, 0, 0, 2]);
    assert_eq!(
        projected_surface_triangles(&[north, south], camera).len(),
        4
    );
}

#[wasm_bindgen_test]
fn mesh_lod_covers_a_translated_viewport_without_first_tile_truncation() {
    let camera = SceneCamera {
        center: [137.0, 119.0],
        zoom: 0.25,
        viewport: [1_280.0, 720.0],
        focus_elevation_meters: 0.0,
    };
    let terrain = (0..256)
        .flat_map(|y| {
            (0..256).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
            })
        })
        .collect::<Vec<_>>();
    let triangles = projected_surface_triangles(&terrain, camera);
    let tile_count = triangles
        .iter()
        .filter(|triangle| !triangle.skirt)
        .map(|triangle| triangle.tile)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert!(tile_count <= MAX_SURFACE_TILES);
    for y in (16..camera.viewport[1] as usize - 16).step_by(32) {
        for x in (16..camera.viewport[0] as usize - 16).step_by(32) {
            let point = ScreenPoint {
                x: x as f64,
                y: y as f64,
            };
            assert!(
                triangles
                    .iter()
                    .filter(|triangle| !triangle.skirt)
                    .any(|triangle| barycentric(triangle.points, point).is_some()),
                "no terrain at {x},{y}"
            );
        }
    }
    let mut with_offscreen = terrain;
    with_offscreen.extend((0..1_000).map(|offset| SceneTerrain {
        position: [10_000.5 + f64::from(offset), 10_000.5],
        material: 2,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
    }));
    let populated = projected_surface_triangles(&with_offscreen, camera);
    assert_eq!(populated.len(), triangles.len());
    assert!(!triangles.is_empty());
    assert_eq!(populated[0].points[0].screen, triangles[0].points[0].screen);
}

#[wasm_bindgen_test]
fn overlapping_surface_pick_uses_interpolated_world_depth() {
    let camera = SceneCamera {
        center: [1.0, 1.0],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let tile = |position: [f64; 2], height: i16| SceneTerrain {
        position,
        material: 0,
        elevation_meters: f64::from(height),
        surface: SceneTerrainSurface::flat(f64::from(height)),
    };
    let triangles =
        projected_surface_triangles(&[tile([0.5, 0.5], 0), tile([1.5, 1.5], 2)], camera);
    let picked = pick_surface_point(&triangles, ScreenPoint { x: 128.0, y: 32.0 });
    assert_eq!(picked, Some([1.5, 1.5]));
    let depths = triangles
        .iter()
        .map(|triangle| {
            triangle
                .points
                .iter()
                .map(|point| point.world[0] + point.world[1])
                .sum::<f64>()
                / 3.0
        })
        .collect::<Vec<_>>();
    assert!(depths.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[wasm_bindgen_test]
fn water_mesh_keeps_the_authoritative_surface_height() {
    let camera = SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 4.0,
    };
    let water = SceneTerrain {
        position: [0.5, 0.5],
        material: 5,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: [4, 4, 4, 4],
            kind: SceneTerrainSurface::PLATEAU,
            triangulation: 0,
            water: 1,
        },
    };
    let triangles = projected_surface_triangles(&[water], camera);
    assert_eq!(triangles.len(), 2);
    assert!(
        triangles
            .iter()
            .flat_map(|triangle| triangle.points)
            .all(|point| { (point.world[2] - 4.0).abs() < f64::EPSILON })
    );
}
