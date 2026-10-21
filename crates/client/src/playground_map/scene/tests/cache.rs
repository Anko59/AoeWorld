use super::*;
use aoe_core::{EntityId, PlayerId, TileRect};
use aoe_protocol::{BuildingKind, ConstructionState, GameplayBuildingState};
use wasm_bindgen_test::wasm_bindgen_test;

thread_local! {
    pub(super) static KEY_ENTRIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[wasm_bindgen_test]
async fn stationary_wall_corner_projections_reuse_the_prepared_foundation_key() {
    let empty = PreparedScene::new(
        Camera {
            center: [0.5; 2],
            zoom: 1.0,
            viewport: [800.0, 600.0],
            focus_elevation_meters: 0.0,
        },
        Vec::new(),
    );
    let mut client = review_tests::client(&empty).await;
    client.village.buildings = (0..192)
        .map(|id| GameplayBuildingState {
            id: EntityId(id),
            player: PlayerId(1),
            age: aoe_protocol::Age::Dark,
            kind: BuildingKind::House,
            footprint: TileRect::from_xywh(id as i32 * 4, 0, 2, 2),
            construction: ConstructionState::Complete,
            progress_permille: 1000,
            hp: 550,
            max_hp: 550,
            los: 4,
            food_remaining: None,
            research: None,
            training_queue: Vec::new(),
            last_hit: None,
            gate_open: false,
        })
        .collect();
    let scene = prepare(&client);
    KEY_ENTRIES.set(0);
    let clock = web_sys::window().unwrap().performance().unwrap();
    let started = clock.now();
    for _ in 0..64 * 4 {
        assert!(Rc::ptr_eq(&scene, &prepare(&client)));
    }
    wasm_bindgen_test::console_log!(
        "192 buildings, 256 cached projections: {} ms, {} key entries",
        clock.now() - started,
        KEY_ENTRIES.get()
    );
    assert_eq!(
        KEY_ENTRIES.get(),
        0,
        "cache hits must not reconstruct 49,152 foundation entries"
    );
    let mut buildings = client.village.buildings.clone();
    buildings[0].hp -= 1;
    replace_buildings(&mut client, buildings.clone());
    assert!(
        Rc::ptr_eq(&scene, &prepare(&client)),
        "health does not change foundations"
    );
    buildings[0].construction = ConstructionState::UnderConstruction;
    replace_buildings(&mut client, buildings.clone());
    let foundation = prepare(&client);
    assert!(
        !Rc::ptr_eq(&scene, &foundation),
        "construction changes the foundation kind"
    );
    buildings[0].footprint = TileRect::from_xywh(3, 4, 2, 2);
    replace_buildings(&mut client, buildings.clone());
    let moved = prepare(&client);
    assert!(
        !Rc::ptr_eq(&foundation, &moved),
        "replacement positions invalidate ground"
    );
    buildings.clear();
    replace_buildings(&mut client, buildings);
    assert!(
        !Rc::ptr_eq(&moved, &prepare(&client)),
        "removal invalidates ground"
    );
}

#[wasm_bindgen_test]
fn building_family_uses_its_observed_age_including_construction() {
    for (kind, dark, feudal) in [
        (BuildingKind::TownCenter, 109, 71),
        (BuildingKind::House, 70, 463),
        (BuildingKind::Mill, 68, 129),
        (BuildingKind::LumberCamp, 562, 563),
        (BuildingKind::MiningCamp, 584, 585),
        (BuildingKind::Barracks, 12, 498),
        (BuildingKind::Dock, 45, 133),
    ] {
        let mut building = GameplayBuildingState {
            id: EntityId(1),
            player: PlayerId(1),
            age: aoe_protocol::Age::Dark,
            kind,
            footprint: TileRect::from_xywh(0, 0, 3, 3),
            construction: ConstructionState::Complete,
            progress_permille: 1000,
            hp: 100,
            max_hp: 100,
            los: 4,
            food_remaining: None,
            research: None,
            training_queue: Vec::new(),
            last_hit: None,
            gate_open: false,
        };
        assert_eq!(building_kind(&building).building_id(), Some(dark));
        building.age = aoe_protocol::Age::Feudal;
        assert_eq!(building_kind(&building).building_id(), Some(feudal));
        building.construction = ConstructionState::UnderConstruction;
        building.progress_permille = 100;
        assert_eq!(building_kind(&building).building_id(), Some(feudal));
    }
}
