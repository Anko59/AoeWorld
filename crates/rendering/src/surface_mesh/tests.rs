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
fn textured_terrain_uses_shared_corner_uvs_and_material_atlas_groups() {
    let frame = |uv| GameFrame {
        uv,
        size: [96.0, 48.0],
        anchor: [48.0, 24.0],
    };
    let mut art = test_art(frame([0.0, 0.0, 0.05, 0.05]));
    art.terrain[3] = vec![frame([0.2, 0.3, 0.04, 0.06])];
    art.terrain[4] = vec![frame([0.4, 0.5, 0.03, 0.02])];
    let sample = SceneTerrain {
        position: [0.5, 0.5],
        material: 3,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: [0, 1, 2, 3],
            kind: SceneTerrainSurface::RAMP,
            triangulation: 0,
            water: 1,
        },
    };
    let camera = SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let mut triangles = projected_surface_triangles(&[sample], camera);
    apply_terrain_textures(&mut triangles, &art);
    assert_eq!(triangles.len(), 2);
    assert!(triangles.iter().all(|triangle| {
        triangle.texture_uv == Some([0.2, 0.3, 0.04, 0.06]) && triangle.tint == 4
    }));
    assert_eq!(
        triangle_texture_coordinates(0),
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]
    );
    assert_eq!(
        triangle_texture_coordinates(1),
        [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    );
    assert_eq!(
        triangle_texture_coordinates(2),
        [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]
    );
    assert_eq!(
        triangle_texture_coordinates(3),
        [[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    );
}

#[wasm_bindgen_test]
fn canvas_texture_affine_mapping_hits_all_projected_triangle_corners() {
    let points = [
        SurfacePoint {
            world: [0.0; 3],
            screen: ScreenPoint { x: 14.0, y: 26.0 },
        },
        SurfacePoint {
            world: [0.0; 3],
            screen: ScreenPoint { x: 82.0, y: 18.0 },
        },
        SurfacePoint {
            world: [0.0; 3],
            screen: ScreenPoint { x: 67.0, y: 73.0 },
        },
    ];
    for mode in 0..=5 {
        let uv = triangle_texture_coordinates(mode);
        let transform = texture_transform(points, uv);
        for (coordinate, point) in uv.into_iter().zip(points) {
            let projected = [
                transform[0] * coordinate[0] + transform[2] * coordinate[1] + transform[4],
                transform[1] * coordinate[0] + transform[3] * coordinate[1] + transform[5],
            ];
            assert!((projected[0] - point.screen.x).abs() < 1e-9);
            assert!((projected[1] - point.screen.y).abs() < 1e-9);
        }
    }
}

#[wasm_bindgen_test]
fn frontmost_surface_depth_includes_elevation_and_cliff_faces() {
    let camera = SceneCamera {
        center: [1.0, 1.0],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let ground = SceneTerrain {
        position: [0.5, 0.5],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
    };
    let high = SceneTerrain {
        position: [1.5, 1.5],
        material: 4,
        elevation_meters: 2.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: [2, 2, 2, 2],
            kind: SceneTerrainSurface::CLIFF,
            triangulation: 0,
            water: 0,
        },
    };
    let triangles = projected_surface_triangles(&[ground, high], camera);
    let screen = ScreenPoint { x: 128.0, y: 32.0 };
    assert_eq!(surface_depth_at(&triangles, screen), Some(7.0));
    assert_eq!(pick_surface_point(&triangles, screen), None);
}
