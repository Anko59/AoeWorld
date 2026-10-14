use super::*;
use aoe_rendering::projected_surface_triangles;

fn ridge(width: i32, height: i32, boundary: i32) -> Vec<SceneTerrain> {
    (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                let level = if x < boundary { 8 } else { 0 };
                SceneTerrain {
                    position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                    material: 7,
                    appearance: None,
                    elevation_meters: f64::from(level),
                    surface: SceneTerrainSurface {
                        corner_game_height_levels: [level; 4],
                        triangulation: 0,
                        water: 0,
                        kind: if (boundary - 3..boundary).contains(&x) {
                            SceneTerrainSurface::CLIFF_LEDGE
                        } else {
                            SceneTerrainSurface::PLATEAU
                        },
                    },
                }
            })
        })
        .collect()
}

fn bounded(terrain: &[SceneTerrain], camera: SceneCamera) -> Vec<ProjectedSurfaceTriangle> {
    let mesh = projected_surface_triangles(terrain, camera);
    assert!(
        mesh.len() <= 24_576,
        "ground plus skirts exceed bound: {} triangles, capacity {}",
        mesh.len(),
        mesh.capacity()
    );
    assert_eq!(
        mesh.capacity(),
        24_576,
        "the reserved triangle vector never grows"
    );
    assert!(!mesh.iter().all(|t| t.skirt), "ground stays present");
    mesh
}

#[wasm_bindgen_test]
fn review_f2_wide_coarse_scene_budgets_all_ninety_six_skirt_triangles() {
    let mesh = bounded(
        &ridge(128, 96, 63),
        SceneCamera {
            center: [64.0, 48.0],
            zoom: 0.25,
            viewport: [4096.0, 2160.0],
            focus_elevation_meters: 4.0,
        },
    );
    assert_eq!(mesh.iter().filter(|t| t.skirt).count(), 96);
    assert!(
        mesh.iter()
            .filter(|t| t.skirt)
            .any(|t| t.points.iter().any(|p| p.world[2] == 8.0))
    );
}

#[wasm_bindgen_test]
fn review_f3_generated_mountain_rectangle_keeps_its_coarse_cliff_inside_budget() {
    let generator = fixture::plane(-432, 0);
    let terrain = fixture::region(&generator, 96..192, 576..704);
    let ledge = terrain
        .iter()
        .find(|s| s.position == [128.5, 600.5])
        .unwrap();
    assert_eq!(ledge.surface.kind, SceneTerrainSurface::CLIFF_LEDGE);
    assert_eq!(ledge.surface.corner_game_height_levels, [184; 4]);
    let eastern = terrain
        .iter()
        .find(|s| s.position == [130.5, 600.5])
        .unwrap();
    assert_eq!(eastern.surface.corner_game_height_levels[0], 183);
    let mesh = bounded(
        &terrain,
        SceneCamera {
            center: [144.0, 640.0],
            zoom: 0.25,
            viewport: [4096.0, 2160.0],
            focus_elevation_meters: 179.0,
        },
    );
    assert!(
        mesh.iter().any(|t| t.skirt
            && t.tile == [128, 600]
            && t.points.iter().any(|p| p.world[2] == 184.0)),
        "generated ledge boundary must stay drawn"
    );
}

#[wasm_bindgen_test]
fn review_f4_tall_coarse_scene_budgets_all_one_hundred_twenty_eight_skirt_triangles() {
    let mesh = bounded(
        &ridge(96, 128, 48),
        SceneCamera {
            center: [48.0, 64.0],
            zoom: 0.25,
            viewport: [3840.0, 2160.0],
            focus_elevation_meters: 2.0,
        },
    );
    assert_eq!(mesh.iter().filter(|t| t.skirt).count(), 128);
    assert!(
        mesh.iter()
            .filter(|t| t.skirt)
            .any(|t| t.points.iter().any(|p| p.world[2] == 8.0))
    );
}
