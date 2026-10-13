use super::shore_tests::{camera, terrain};
use super::*;

#[wasm_bindgen_test]
fn river_bank_corners_round_with_identical_land_and_water_vertices() {
    for (material, ramp, concave) in [
        (10, false, false),
        (12, false, false),
        (10, true, false),
        (12, true, false),
        (10, false, true),
        (12, false, true),
        (10, true, true),
        (12, true, true),
    ] {
        let samples = (0..4)
            .flat_map(|y| {
                (0..4).map(move |x| {
                    let wet = (x < 2 && y < 2) != concave;
                    let mut sample = terrain([x, y], if wet { 3 } else { 0 }, 0);
                    sample.material = if wet { material } else { 2 };
                    if ramp {
                        let h = (x + y) as i16;
                        sample.surface.corner_game_height_levels = [h, h + 1, h + 2, h + 1];
                        sample.surface.kind = SceneTerrainSurface::RAMP;
                    }
                    sample
                })
            })
            .collect::<Vec<_>>();
        let mut test_camera = camera();
        test_camera.center = [2.0, 2.0];
        test_camera.viewport = [640.0, 480.0];
        let triangles = projected_surface_triangles(&samples, test_camera);
        let height = if ramp { 4.0 } else { 0.0 };
        for tile in [[1, 1], [2, 1], [1, 2], [2, 2]] {
            assert!(
                triangles
                    .iter()
                    .filter(|t| t.tile == tile)
                    .flat_map(|t| t.points)
                    .any(|p| p.world == [1.75, 1.75, height]),
                "unrounded river/Ford corner on {tile:?}"
            );
        }
        assert!(
            triangles
                .iter()
                .flat_map(|t| t.points)
                .all(|p| p.world != [2.0, 2.0, height])
        );
        // Reordering source tiles must not open a visible crack.
        let mut reverse = samples;
        reverse.reverse();
        let reversed = projected_surface_triangles(&reverse, test_camera);
        assert_eq!(
            triangles
                .iter()
                .flat_map(|t| t.points)
                .map(|p| p.world)
                .collect::<Vec<_>>(),
            reversed
                .iter()
                .flat_map(|t| t.points)
                .map(|p| p.world)
                .collect::<Vec<_>>()
        );
    }
}

#[wasm_bindgen_test]
fn river_bank_bevel_preserves_straight_diagonal_cliff_height_and_halo_boundaries() {
    for case in 0..5 {
        let mut samples = (0..4)
            .flat_map(|y| {
                (0..4).filter_map(move |x| {
                    if case == 4 && [x, y] == [1, 2] {
                        return None;
                    }
                    let wet = match case {
                        0 => x < 2,
                        1 => (x < 2) == (y < 2),
                        _ => x < 2 && y < 2,
                    };
                    let mut sample = terrain([x, y], if wet { 3 } else { 0 }, 0);
                    sample.material = if wet { 10 } else { 2 };
                    if [x, y] == [1, 2] {
                        if case == 2 {
                            sample.surface.kind = SceneTerrainSurface::CLIFF;
                        } else if case == 3 {
                            sample.surface.corner_game_height_levels = [1; 4];
                        }
                    }
                    Some(sample)
                })
            })
            .collect::<Vec<_>>();
        let mut test_camera = camera();
        test_camera.center = [2.0, 2.0];
        test_camera.viewport = [640.0, 480.0];
        for _ in 0..2 {
            let triangles = projected_surface_triangles(&samples, test_camera);
            let points = triangles.iter().filter(|t| !t.skirt).flat_map(|t| t.points);
            assert!(
                points.clone().any(|p| p.world[..2] == [2.0, 2.0]),
                "case {case}"
            );
            assert!(
                points.clone().all(|p| p.world[..2] != [1.75, 1.75]),
                "case {case}"
            );
            samples.reverse();
        }
    }
}
