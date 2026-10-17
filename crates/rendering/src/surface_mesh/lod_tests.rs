use super::*;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

fn variable_height_map(side: i32) -> Vec<SceneTerrain> {
    let vertex_height = |x: i32, y: i32| ((x * 3 + y * 5).rem_euclid(9)) as i16;
    (0..side)
        .flat_map(|y| {
            (0..side).map(move |x| {
                let corners = [
                    vertex_height(x, y),
                    vertex_height(x + 1, y),
                    vertex_height(x + 1, y + 1),
                    vertex_height(x, y + 1),
                ];
                let triangulation = ((x + y) & 1) as u8;
                let elevation_meters = sample_surface_height(corners, triangulation, 0.5, 0.5);
                SceneTerrain {
                    appearance: None,
                    position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                    material: 0,
                    elevation_meters,
                    surface: SceneTerrainSurface {
                        corner_game_height_levels: corners,
                        kind: SceneTerrainSurface::RAMP,
                        triangulation,
                        water: 0,
                    },
                }
            })
        })
        .collect()
}

#[wasm_bindgen_test]
fn elevated_variable_lod_covers_view_and_uses_shared_coarse_corners() {
    let camera = SceneCamera {
        center: [256.0, 256.0],
        zoom: 0.25,
        viewport: [4_096.0, 2_160.0],
        focus_elevation_meters: 4.0,
    };
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let terrain = variable_height_map(512);
    let visible_count = terrain
        .iter()
        .filter(|sample| {
            let tile = tile_key(sample.position);
            fine_tile_visible(&projection, **sample, tile, camera.viewport)
        })
        .count();
    assert!(visible_count > MAX_SURFACE_TILES);

    let triangles = projected_surface_triangles(&terrain, camera);
    let top = triangles
        .iter()
        .filter(|triangle| !triangle.skirt)
        .collect::<Vec<_>>();
    let cell_count = top
        .iter()
        .map(|triangle| triangle.tile)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert!(cell_count <= MAX_SURFACE_TILES);
    assert!(cell_count < visible_count);

    let mut vertex_heights = std::collections::BTreeMap::new();
    for triangle in &top {
        for point in triangle.points {
            let key = (point.world[0].round() as i32, point.world[1].round() as i32);
            if let Some(previous) = vertex_heights.insert(key, point.world[2]) {
                assert!((previous - point.world[2]).abs() < f64::EPSILON);
            }
        }
    }
    assert!(vertex_heights.values().any(|height| *height > 0.0));
    for y in (96..camera.viewport[1] as usize - 96).step_by(192) {
        for x in (96..camera.viewport[0] as usize - 96).step_by(192) {
            let screen = ScreenPoint {
                x: x as f64,
                y: y as f64,
            };
            assert!(
                top.iter()
                    .any(|triangle| barycentric(triangle.points, screen).is_some()),
                "raised map LOD left a terrain hole at {x},{y}"
            );
        }
    }
}

#[wasm_bindgen_test]
fn authoritative_zero_corners_do_not_create_a_fake_cliff_skirt() {
    let camera = SceneCamera {
        center: [1.0, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let cliff = |position, elevation_meters| SceneTerrain {
        appearance: None,
        position,
        material: 4,
        elevation_meters,
        surface: SceneTerrainSurface {
            corner_game_height_levels: [0; 4],
            kind: SceneTerrainSurface::CLIFF,
            triangulation: 0,
            water: 0,
        },
    };
    let triangles =
        projected_surface_triangles(&[cliff([0.5, 0.5], 8.0), cliff([1.5, 0.5], 0.0)], camera);
    assert_eq!(triangles.len(), 4);
    assert!(triangles.iter().all(|triangle| !triangle.skirt));
}

#[wasm_bindgen_test]
fn a_frontmost_cliff_does_not_cancel_pickable_ground_behind_it() {
    let camera = SceneCamera {
        center: [1.0, 1.0],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let ground = SceneTerrain {
        appearance: None,
        position: [0.5, 0.5],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
    };
    let cliff = SceneTerrain {
        appearance: None,
        position: [1.5, 1.5],
        material: 4,
        elevation_meters: 2.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: [2; 4],
            kind: SceneTerrainSurface::CLIFF,
            triangulation: 0,
            water: 0,
        },
    };
    let triangles = projected_surface_triangles(&[ground, cliff], camera);
    assert_eq!(
        super::super::pick_surface_point(&triangles, ScreenPoint { x: 128.0, y: 32.0 }),
        Some([0.5, 0.5])
    );
}

#[wasm_bindgen_test]
fn desktop_minimum_zoom_keeps_source_shores_and_elevations() {
    let terrain = variable_height_map(256);
    for viewport in [[1280.0, 720.0], [1920.0, 1080.0]] {
        let triangles = projected_surface_triangles(
            &terrain,
            SceneCamera {
                center: [128.0, 128.0],
                zoom: 0.25,
                viewport,
                focus_elevation_meters: 4.0,
            },
        );
        assert!(triangles.len() <= MAX_SURFACE_TRIANGLES);
        for face in triangles.iter().filter(|t| !t.skirt) {
            let sample = &terrain[(face.tile[1] * 256 + face.tile[0]) as usize];
            for point in face.points {
                assert!((point.world[0] - f64::from(face.tile[0])).abs() <= 1.0);
                assert!((point.world[1] - f64::from(face.tile[1])).abs() <= 1.0);
                let expected = sample_surface_height(
                    sample.surface.corner_game_height_levels,
                    sample.surface.triangulation,
                    point.world[0] - f64::from(face.tile[0]),
                    point.world[1] - f64::from(face.tile[1]),
                );
                assert_eq!(
                    point.world[2], expected,
                    "source elevation changed at minimum zoom"
                );
            }
        }
    }
}

#[wasm_bindgen_test]
fn minimum_zoom_keeps_the_previous_levels_jagged_shore_and_ground_sheet() {
    let mut terrain = variable_height_map(256);
    for sample in &mut terrain {
        let [x, y] = sample.position.map(|v| v.floor() as i32);
        if x < 128 + (y % 7) - 3 {
            sample.material = 5;
            sample.surface = SceneTerrainSurface::flat(2.0);
            sample.surface.water = 1;
        }
    }
    for zoom in [0.25, 0.5] {
        let triangles = projected_surface_triangles(
            &terrain,
            SceneCamera {
                center: [128.0, 128.0],
                zoom,
                viewport: [1280.0, 720.0],
                focus_elevation_meters: 4.0,
            },
        );
        assert!(!triangles.is_empty());
        assert!(triangles.len() <= MAX_SURFACE_TRIANGLES);
        let mut shore = 0;
        for face in triangles.iter().filter(|face| !face.skirt) {
            let sample = &terrain[(face.tile[1] * 256 + face.tile[0]) as usize];
            assert_eq!(face.material, sample.material);
            for point in face.points {
                let local = [
                    point.world[0] - f64::from(face.tile[0]),
                    point.world[1] - f64::from(face.tile[1]),
                ];
                assert!(local.into_iter().all(|v| (0.0..=1.0).contains(&v)));
                assert_eq!(
                    point.world[2],
                    sample_surface_height(
                        sample.surface.corner_game_height_levels,
                        sample.surface.triangulation,
                        local[0],
                        local[1]
                    )
                );
            }
            shore += usize::from(sample.surface.water != 0);
        }
        assert!(shore > 100);
    }
}
