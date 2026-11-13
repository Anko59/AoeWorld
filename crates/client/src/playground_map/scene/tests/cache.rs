use super::*;
use aoe_core::{EntityId, PlayerId, TileRect};
use aoe_protocol::{BuildingKind, ConstructionState, GameplayBuildingState};
use wasm_bindgen_test::wasm_bindgen_test;

thread_local! {
    pub(super) static KEY_ENTRIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[wasm_bindgen_test]
async fn prepared_terrain_plan_survives_actor_updates_and_follows_view_replacement() {
    let empty = PreparedScene::new(
        Camera {
            center: [8.0; 2],
            zoom: 1.0,
            viewport: [320.0; 2],
            focus_elevation_meters: 0.0,
        },
        Vec::new(),
    );
    let mut client = review_tests::client(&empty).await;
    let scene = prepare(&client);
    let unplanned = scene.resident_bytes();
    let plan = scene.terrain_plan(&mut client.art);
    let identity = &*plan as *const aoe_rendering::PreparedTerrain;
    drop(plan);
    assert!(
        scene.resident_bytes() > unplanned,
        "retained geometry is accounted"
    );
    for _ in 0..16 {
        client.units.values_mut().next().unwrap().hp -= 1;
        assert!(Rc::ptr_eq(&scene, &prepare(&client)));
        let current = scene.terrain_plan(&mut client.art);
        assert!(std::ptr::eq(identity, &*current));
    }
    client.camera.center[0] += 32.0;
    let moved = prepare(&client);
    assert!(!Rc::ptr_eq(&scene, &moved));
    let current = moved.terrain_plan(&mut client.art);
    assert!(!std::ptr::eq(identity, &*current));
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn diagnostic_world_keeps_ground_when_fog_is_disabled_and_no_map_chunks_exist() {
    let scene = PreparedScene::new(
        Camera {
            center: [8.0; 2],
            zoom: 1.0,
            viewport: [320.0; 2],
            focus_elevation_meters: 0.0,
        },
        Vec::new(),
    );
    let mut client = review_tests::client(&scene).await;
    client.terrain_chunks.clear();
    client.map_content_hash = None;
    assert!(client.fog.apply(aoe_protocol::FogState {
        revision: 1,
        tick: aoe_core::Tick(0),
        region: TileRect::from_xywh(0, 0, 16, 16),
        disabled: true,
        tiles: Vec::new(),
        overview: Vec::new(),
    }));
    let ground = super::super::scene_terrain(&client);
    assert!(
        !ground.is_empty(),
        "native DDS terrain needs ground geometry and residency demand"
    );
    assert!(
        ground
            .iter()
            .all(|tile| tile.material == 0 && tile.elevation_meters == 0.0)
    );
    client.map_content_hash = Some([1; 32]);
    assert!(
        super::super::scene_terrain(&client).is_empty(),
        "a geographic map must wait for its own terrain"
    );
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
            rally_point: None,
            farm_reseeds: 0,
            garrison: Vec::new(),
            bridge_span: None,
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
    buildings[0].age = aoe_protocol::Age::Imperial;
    replace_buildings(&mut client, buildings.clone());
    let aged = prepare(&client);
    assert!(
        !Rc::ptr_eq(&moved, &aged),
        "an observed age update invalidates the construction identity"
    );
    buildings.clear();
    replace_buildings(&mut client, buildings);
    assert!(
        !Rc::ptr_eq(&aged, &prepare(&client)),
        "removal invalidates ground"
    );
}

#[wasm_bindgen_test]
fn building_family_uses_its_observed_age_including_construction() {
    for (kind, ids) in [
        (BuildingKind::TownCenter, [109, 71, 141, 142]),
        (BuildingKind::House, [70, 463, 464, 465]),
        (BuildingKind::Mill, [68, 129, 130, 131]),
        (BuildingKind::LumberCamp, [562, 563, 564, 565]),
        (BuildingKind::MiningCamp, [584, 585, 586, 587]),
        (BuildingKind::Barracks, [12, 498, 132, 20]),
        (BuildingKind::Dock, [45, 133, 47, 51]),
        (BuildingKind::ArcheryRange, [87, 87, 10, 14]),
        (BuildingKind::Stable, [101, 101, 86, 153]),
        (BuildingKind::Blacksmith, [103, 103, 18, 19]),
        (BuildingKind::Market, [84, 84, 116, 137]),
        (BuildingKind::Monastery, [104, 104, 31, 32]),
        (BuildingKind::University, [209, 209, 209, 210]),
        (BuildingKind::SiegeWorkshop, [49, 49, 49, 150]),
        (BuildingKind::Castle, [82, 82, 82, 82]),
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
            rally_point: None,
            farm_reseeds: 0,
            garrison: Vec::new(),
            bridge_span: None,
        };
        for (age, id) in [
            aoe_protocol::Age::Dark,
            aoe_protocol::Age::Feudal,
            aoe_protocol::Age::Castle,
            aoe_protocol::Age::Imperial,
        ]
        .into_iter()
        .zip(ids)
        {
            building.age = age;
            building.construction = ConstructionState::Complete;
            building.progress_permille = 1000;
            assert_eq!(
                building_kind(&building).building_id(),
                Some(id),
                "complete {kind:?}, {age:?}"
            );
            building.construction = ConstructionState::UnderConstruction;
            building.progress_permille = 100;
            assert_eq!(
                building_kind(&building).building_id(),
                Some(id),
                "foundation {kind:?}, {age:?}"
            );
        }
    }
}

#[wasm_bindgen_test]
async fn offscreen_foundation_changes_preserve_the_visible_mesh() {
    let mut client = review_tests::picking::fixture(1.0).await;
    client.fog.apply(aoe_protocol::FogState {
        revision: 1,
        tick: aoe_core::Tick(0),
        region: TileRect::from_xywh(0, 0, 1, 1),
        tiles: vec![],
        overview: vec![],
        disabled: true,
    });
    let scene = prepare(&client);
    assert!(!scene.terrain.is_empty());
    let mut distant = client.village.buildings[0].clone();
    distant.id = EntityId(999);
    distant.footprint = TileRect::from_xywh(10000, 10000, 4, 4);
    let mut buildings = client.village.buildings.clone();
    buildings.push(distant);
    replace_buildings(&mut client, buildings.clone());
    assert!(
        Rc::ptr_eq(&scene, &prepare(&client)),
        "offscreen additions cannot repaint displayed terrain"
    );
    buildings[1].age = aoe_protocol::Age::Imperial;
    replace_buildings(&mut client, buildings.clone());
    assert!(
        Rc::ptr_eq(&scene, &prepare(&client)),
        "offscreen age changes cannot repaint displayed terrain"
    );
    buildings[1].footprint = TileRect::from_xywh(102, 102, 4, 4);
    replace_buildings(&mut client, buildings);
    assert!(
        !Rc::ptr_eq(&scene, &prepare(&client)),
        "entering the displayed ground must invalidate it"
    );
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn offscreen_native_foundation_apron_still_invalidates_visible_ground() {
    let mut client = review_tests::picking::fixture(1.0).await;
    client.art.compositions[0].footprint = [16.0; 2];
    client.fog.apply(aoe_protocol::FogState {
        revision: 1,
        tick: aoe_core::Tick(0),
        region: TileRect::from_xywh(0, 0, 1, 1),
        tiles: vec![],
        overview: vec![],
        disabled: true,
    });
    let scene = prepare(&client);
    let edge = scene
        .terrain
        .iter()
        .map(|t| t.position[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let mut near = client.village.buildings[0].clone();
    near.id = EntityId(998);
    near.footprint = TileRect::from_xywh(edge.ceil() as i32 + 3, 98, 4, 4);
    let mut buildings = client.village.buildings.clone();
    buildings.push(near);
    replace_buildings(&mut client, buildings);
    assert!(
        !Rc::ptr_eq(&scene, &prepare(&client)),
        "native apron reaches displayed samples even with its centre outside"
    );
    client.canvas.remove();
}
