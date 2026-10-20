use super::*;
use aoe_core::{EntityId, ScreenPoint};
use wasm_bindgen_test::wasm_bindgen_test;

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
