use super::*;
use aoe_core::{EntityId, ScreenPoint};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn farm_crop_uvs_keep_both_triangle_halves_when_the_apron_blends() {
    let mut art = crate::surface_mesh::mix::fixture::art();
    art.buildings[5] = vec![art.terrain[2][0]];
    art.compositions.push(crate::BuildingArt {
        unit_id: 50,
        footprint: [3.0; 2],
        foundation_material: Some(17),
        parts: Vec::new(),
        construction: Vec::new(),
    });
    let camera = SceneCamera {
        center: [0.5; 2],
        zoom: 1.0,
        viewport: [800.0, 600.0],
        focus_elevation_meters: 0.0,
    };
    let farm_unit = SceneUnit {
        id: EntityId(1),
        kind: SceneUnitKind::Farm,
        position: [0.5; 2],
        elevation_meters: 0.0,
        selected: false,
        moving: false,
        facing: 0,
    };
    let mut tiles = (-3..=3)
        .flat_map(|y| {
            (-3..=3).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance: Some(SceneTerrainAppearance {
                    canopy_strength: 0,
                    floor_strength: 0,
                    palette: 0,
                    exposure: 0,
                    height_band: 0,
                }),
            })
        })
        .collect::<Vec<_>>();
    apply_building_foundations(&mut tiles, &art, &[farm_unit]);
    let mut faces = projected_surface_triangles(&tiles, camera);
    let modes = faces.iter().map(|f| f.mode()).collect::<Vec<_>>();
    apply_terrain_textures(&mut faces, &art);
    let ground = BuildingGround::new(&[farm_unit], camera);
    let mut checked = 0;
    for (face, mode) in faces.iter_mut().zip(modes) {
        let center =
            [0, 1].map(|axis| face.points.iter().map(|p| p.world[axis]).sum::<f64>() / 3.0);
        if !face.skirt && center.iter().all(|v| (*v - 0.5).abs() < 1.5) {
            ground.farm(face, &art);
            assert_eq!(face.mode(), mode, "crop UV orientation at {:?}", face.tile);
            checked += 1;
        }
    }
    assert_eq!(checked, 18);
}

#[wasm_bindgen_test]
fn distant_farms_do_not_enter_the_per_triangle_candidates() {
    let camera = SceneCamera {
        center: [256.0; 2],
        zoom: 1.0,
        viewport: [800.0, 600.0],
        focus_elevation_meters: 0.0,
    };
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let farm = |id, position, elevation_meters| SceneUnit {
        id: EntityId(id),
        kind: SceneUnitKind::Farm,
        position,
        elevation_meters,
        selected: false,
        moving: false,
        facing: 0,
    };
    let mut units: Vec<_> = (0..100)
        .map(|id| farm(id, [1000.0 + f64::from(id) * 4.0; 2], 0.0))
        .collect();
    assert!(
        BuildingGround::new(&units, camera).farms.is_empty(),
        "100 distant farms must cause zero farm predicates per terrain triangle"
    );
    units.push(farm(100, camera.center, 0.0));
    // A footprint whose center is offscreen can still cover visible ground.
    units.push(farm(
        101,
        projection.screen_to_world(ScreenPoint {
            x: -190.0,
            y: 300.0,
        }),
        0.0,
    ));
    units.push(farm(
        102,
        projection.screen_to_world(ScreenPoint {
            x: -194.0,
            y: 300.0,
        }),
        0.0,
    ));
    // Height-aware projection keeps a raised footprint that reaches the view.
    units.push(farm(
        103,
        projection.screen_to_world_at_height(ScreenPoint { x: 400.0, y: 300.0 }, 30.0),
        30.0,
    ));
    let ground = BuildingGround::new(&units, camera);
    assert_eq!(ground.farms.len(), 3);
    assert!(ground.farms.contains(&units[101].position));
    assert!(!ground.farms.contains(&units[102].position));
    assert!(ground.farms.contains(&units[103].position));
}
