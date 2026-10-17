use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn unique_contact_keys_preserve_reference_bucket_triangle_order() {
    let terrain = (0..64)
        .flat_map(|y| {
            (0..64).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                appearance: None,
                elevation_meters: f64::from((x % 4) * 2),
                surface: SceneTerrainSurface::flat(f64::from((x % 4) * 2)),
            })
        })
        .collect();
    let camera = Camera {
        center: [32.0, 32.0],
        zoom: 0.25,
        viewport: [4096.0, 2160.0],
        focus_elevation_meters: 0.0,
    };
    let scene = PreparedScene::new(camera, terrain);
    let mut expected = std::collections::BTreeMap::<_, Vec<_>>::new();
    for (index, triangle) in scene.triangles.iter().enumerate().filter(|(_, t)| !t.skirt) {
        let (min, max) = bounds(triangle);
        for y in bucket(min[1], scene.bucket_width)..=bucket(max[1], scene.bucket_width) {
            for x in bucket(min[0], scene.bucket_width)..=bucket(max[0], scene.bucket_width) {
                expected.entry([x, y]).or_default().push(index);
            }
        }
    }
    assert_eq!(scene.buckets.len(), expected.len());
    assert!(scene.buckets.len() <= scene.triangles.len() * 4);
    assert!(scene.buckets.iter().map(Vec::len).sum::<usize>() <= scene.triangles.len() * 4);
    for (key, indexes) in expected {
        let slot = scene.bucket_keys.get(key).unwrap();
        assert_eq!(scene.buckets[slot], indexes);
    }
}

#[wasm_bindgen_test]
fn contacts_follow_projected_coarse_triangles_instead_of_fine_height() {
    let terrain = (0..128)
        .flat_map(|y| {
            (0..128).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                appearance: None,
                elevation_meters: f64::from((x % 4) * 2),
                surface: SceneTerrainSurface {
                    corner_game_height_levels: [(x % 4 * 2) as i16; 4],
                    kind: 0,
                    triangulation: 0,
                    water: 0,
                },
            })
        })
        .collect();
    let camera = Camera {
        center: [64.0, 64.0],
        zoom: 0.25,
        viewport: [4096.0, 2160.0],
        focus_elevation_meters: 0.0,
    };
    let scene = PreparedScene::new(camera, terrain);
    let mut checked = 0;
    let mut changed = false;
    for triangle in scene.triangles.iter().filter(|t| !t.skirt) {
        let world =
            [0, 1].map(|axis| triangle.points.iter().map(|p| p.world[axis]).sum::<f64>() / 3.0);
        let expected = triangle.points.iter().map(|p| p.world[2]).sum::<f64>() / 3.0;
        assert!((scene.height(world).unwrap() - expected).abs() < 1e-6);
        changed |= (expected - (world[0].floor() % 4.0) * 2.0).abs() > 0.1;
        checked += 1;
    }
    assert!(checked > 100);
    assert!(
        changed,
        "fixture must exercise fine-versus-coarse height disagreement"
    );
    assert!(scene.height([-20.0, -20.0]).is_none());
}
