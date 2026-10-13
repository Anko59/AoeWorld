use super::*;

#[wasm_bindgen_test]
fn crossing_ground_edges_do_not_make_an_unsupported_rock_wedge() {
    let terrain = [
        SceneTerrain {
            position: [0.5, 0.5],
            material: 7,
            elevation_meters: 4.0,
            appearance: None,
            surface: SceneTerrainSurface {
                corner_game_height_levels: [4; 4],
                kind: SceneTerrainSurface::CLIFF_LEDGE,
                triangulation: 0,
                water: 0,
            },
        },
        SceneTerrain {
            position: [1.5, 0.5],
            material: 7,
            elevation_meters: 2.5,
            appearance: None,
            surface: SceneTerrainSurface {
                corner_game_height_levels: [5, 5, 0, 0],
                kind: SceneTerrainSurface::PLATEAU,
                triangulation: 0,
                water: 0,
            },
        },
    ];
    let camera = SceneCamera {
        center: [1.0, 0.5],
        zoom: 1.0,
        viewport: [512.0; 2],
        focus_elevation_meters: 2.0,
    };
    let faces = projected_surface_triangles(&terrain, camera);
    let walls = faces.iter().filter(|f| f.skirt).collect::<Vec<_>>();
    assert_eq!(
        walls.len(),
        2,
        "splitting a crossing preserves the triangle budget"
    );
    for wall in walls {
        for weights in [
            [0.5, 0.5, 0.0],
            [0.0, 0.5, 0.5],
            [0.5, 0.0, 0.5],
            [1.0 / 3.0; 3],
        ] {
            let y = wall
                .points
                .iter()
                .zip(weights)
                .map(|(p, w)| p.world[1] * w)
                .sum::<f64>();
            let z = wall
                .points
                .iter()
                .zip(weights)
                .map(|(p, w)| p.world[2] * w)
                .sum::<f64>();
            let neighboring_ground = 5.0 - 5.0 * y;
            assert!(
                z <= 4.0_f64.max(neighboring_ground) + 0.000001,
                "wall rises above both sheets at {y}: {z}"
            );
            assert!(
                z >= 4.0_f64.min(neighboring_ground) - 0.000001,
                "wall falls below both sheets at {y}: {z}"
            );
        }
    }
}
