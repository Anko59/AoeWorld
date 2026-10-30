use super::*;
use aoe_rendering::{AtlasAddress, GameFrame};

#[wasm_bindgen_test]
async fn movement_cursor_uses_selected_domains_and_accepts_any_in_mixed_selection() {
    use aoe_map::{GroundMaterial, MapChunkGenerator, WaterKind};
    use aoe_protocol::UnitKind;
    let shared = ship_on_water().await;
    let mut client = shared.borrow_mut();
    let mut chunk = MapChunkGenerator::new([0; 32], 1, 1024)
        .landscape_chunk_with_cancel(8, 18, &|| false)
        .unwrap();
    chunk.resources.clear();
    chunk.decorations.clear();
    for (water, material, land_passable) in [
        (WaterKind::Ocean, GroundMaterial::DeepWater, false),
        (WaterKind::None, GroundMaterial::TemperateGrass, true),
        (WaterKind::River, GroundMaterial::Ford, true),
        (WaterKind::Shallow, GroundMaterial::Shore, false),
    ] {
        for cell in &mut chunk.tiles {
            cell.terrain.water = water;
            cell.terrain.material = material;
            cell.terrain.passable = land_passable;
            cell.terrain.game_height_level = 0;
            cell.terrain.surface.corner_game_height_levels = [0; 4];
            cell.terrain.surface.kind = aoe_map::SurfaceKind::Plateau;
        }
        let cached = super::super::super::CachedChunk::from(chunk.clone());
        let terrain = cached
            .scene_tiles()
            .map(|(p, t, a)| super::super::super::terrain_scene_sample(p, t, a))
            .collect();
        *client.terrain_scene.borrow_mut() =
            Some(Rc::new(PreparedScene::new(client.camera, terrain)));
        client.terrain_chunks.insert((8, 18), cached);
        client.pointer = Some(client.camera.world_to_screen([284.5, 603.5]));
        let mut land = client.units[&EntityId(1)];
        land.id = EntityId(2);
        land.kind = UnitKind::Villager;
        client.units.insert(land.id, land);
        for (selection, expected) in [
            // All water is navigable by ships, including the dual-domain Ford.
            (
                vec![EntityId(1)],
                if water != WaterKind::None { 2.0 } else { 3.0 },
            ),
            (vec![EntityId(2)], if land_passable { 2.0 } else { 3.0 }),
            (vec![EntityId(1), EntityId(2)], 2.0),
        ] {
            client.selected.units = selection;
            for move_cursor in [false, true] {
                client.village.move_cursor = move_cursor;
                assert_eq!(
                    crate::playground::game::village::presentation::values(&client)[3],
                    expected,
                    "{water:?} {material:?}, move mode {move_cursor}"
                );
            }
        }
    }
    client.canvas.remove();
}

async fn ship_on_water() -> Rc<RefCell<Client>> {
    let scene = PreparedScene::new(
        Camera {
            center: [282.5, 603.5],
            zoom: 1.0,
            viewport: [512.0; 2],
            focus_elevation_meters: 0.0,
        },
        (600..608)
            .flat_map(|y| {
                (279..287).map(move |x| SceneTerrain {
                    position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                    material: 9,
                    appearance: None,
                    elevation_meters: 0.0,
                    surface: SceneTerrainSurface {
                        water: 1,
                        ..SceneTerrainSurface::flat(0.0)
                    },
                })
            })
            .collect(),
    );
    let mut client = client(&scene).await;
    client.map_content_hash = Some([1; 32]);
    *client.terrain_scene.borrow_mut() = Some(Rc::new(scene));
    let ship = client.units.get_mut(&EntityId(1)).unwrap();
    ship.kind = aoe_protocol::UnitKind::FishingShip;
    ship.facing = 2;
    ship.hp = 50;
    ship.max_hp = 50;
    client.selected = Default::default();
    // Native graphics.drs 444, facing row 2: metadata only, no game pixels.
    let hull = GameFrame {
        atlas: AtlasAddress {
            page: 0,
            uv: [0.0, 0.0, 1.0, 1.0],
        },
        size: [74.0, 80.0],
        anchor: [35.0, 72.0],
    };
    client.art.fishing[7] = vec![hull; 5];
    // All actor rows share this fixture's geometry, including packed phases.
    client.art.actors = vec![vec![hull; 5]; aoe_assets::catalog::units::SPRITES.len()];
    client.canvas.set_width(512);
    client.canvas.set_height(512);
    client
        .canvas
        .set_attribute(
            "style",
            "position:fixed;left:0;top:0;width:512px;height:512px",
        )
        .unwrap();
    client
        .document
        .body()
        .unwrap()
        .append_child(&client.canvas)
        .unwrap();
    let shared = Rc::new(RefCell::new(client));
    crate::playground::game::controls::install(shared.clone()).unwrap();
    shared
}

fn click(shared: &Rc<RefCell<Client>>, point: [f64; 2]) {
    let canvas = shared.borrow().canvas.clone();
    let rect = canvas.get_bounding_client_rect();
    let options = js_sys::Object::new();
    for (key, value) in [
        ("clientX", rect.left() + point[0]),
        ("clientY", rect.top() + point[1]),
        ("button", 0.0),
        ("pointerId", 1.0),
    ] {
        js_sys::Reflect::set(&options, &key.into(), &value.into()).unwrap();
    }
    let constructor: js_sys::Function =
        js_sys::Reflect::get(&web_sys::window().unwrap(), &"PointerEvent".into())
            .unwrap()
            .unchecked_into();
    for name in ["pointerdown", "pointerup"] {
        let event: web_sys::Event =
            js_sys::Reflect::construct(&constructor, &js_sys::Array::of2(&name.into(), &options))
                .unwrap()
                .unchecked_into();
        canvas.dispatch_event(&event).unwrap();
    }
}

#[wasm_bindgen_test]
async fn native_fishing_ship_hull_below_its_anchor_is_selectable_on_water() {
    let shared = ship_on_water().await;
    // The visible hull extends eight native pixels below its water contact.
    click(&shared, [256.0, 262.0]);
    let selected = shared.borrow().selected.units.clone();
    shared.borrow().canvas.remove();
    assert_eq!(
        selected,
        vec![EntityId(1)],
        "flat water must not occlude the hull"
    );
}

#[wasm_bindgen_test]
async fn native_fishing_ship_selection_uses_its_wide_body_bounds() {
    let shared = ship_on_water().await;
    // The native body is wider than the old 32-pixel selection half-width.
    click(&shared, [296.0, 236.0]);
    let selected = shared.borrow().selected.units.clone();
    shared.borrow().canvas.remove();
    assert_eq!(
        selected,
        vec![EntityId(1)],
        "visible native hull must be selectable"
    );
}

#[wasm_bindgen_test]
async fn clicking_fish_on_water_selects_a_resource_and_never_a_unit() {
    use aoe_map::{
        LandscapeChunk, LandscapeResource, ObjectKind, ResourceNode, ResourceVisualFamily,
    };
    use aoe_protocol::ResourceState;
    let shared = ship_on_water().await;
    {
        let mut client = shared.borrow_mut();
        client.units.clear();
        assert!(client.fog.apply(aoe_protocol::FogState {
            revision: 1,
            tick: aoe_core::Tick(0),
            disabled: false,
            region: aoe_core::TileRect::new(
                aoe_core::TileCoord::new(280, 600),
                aoe_core::TileCoord::new(288, 608)
            ),
            tiles: vec![2; 64],
            overview: Vec::new(),
        }));
        assert!(client.resources.apply(ResourceState {
            snapshot_page: None,
            subscription_revision: 1,
            from_revision: None,
            revision: 1,
            changes: Vec::new(),
        }));
        client.terrain_chunks.insert(
            (8, 18),
            LandscapeChunk {
                x: 8,
                y: 18,
                tiles: Vec::new(),
                resources: vec![LandscapeResource {
                    node: ResourceNode {
                        id: 42,
                        tile: aoe_core::TileCoord::new(282, 603),
                        kind: aoe_map::ResourceKind::Food,
                        object: ObjectKind::DeepFish,
                        initial_amount: 225,
                        visual_variant: 0,
                    },
                    visual_family: ResourceVisualFamily::DeepFish,
                }],
                decorations: Vec::new(),
            }
            .into(),
        );
    }
    click(&shared, [256.0, 256.0]);
    let client = shared.borrow();
    client.canvas.remove();
    assert!(client.selected.units.is_empty());
    assert_eq!(client.selected.resource_id, Some(42));
    assert!(units(&client, &prepare(&client)).is_empty());
}

#[wasm_bindgen_test]
async fn living_fishing_ship_remains_selectable_beneath_a_carcass() {
    let shared = ship_on_water().await;
    let carcass = aoe_protocol::AnimalState {
        id: EntityId(2),
        kind: aoe_protocol::AnimalKind::Boar,
        owner: None,
        hp: 0,
        position: WorldPosition::from_tile_center(aoe_core::TileCoord::new(282, 603)).unwrap(),
        task: aoe_protocol::AnimalTask::Idle,
        food_remaining: 100,
        target: None,
        facing: 2,
        moving: false,
        death_tick: Some(aoe_core::Tick(0)),
    };
    shared.borrow_mut().village.animals.push(carcass);
    // The carcass has the frontmost ID at the same projected contact point.
    click(&shared, [256.0, 256.0]);
    let selected = shared.borrow().selected.units.clone();
    shared.borrow().canvas.remove();
    assert_eq!(selected, vec![EntityId(1)]);
}

#[wasm_bindgen_test]
async fn fishing_tasks_share_native_actor_poses_phases_and_gather_orders() {
    use crate::unit_visuals::{ClickOrder, Pose, click_order, packed_frame, unit_pose};
    use aoe_protocol::{UnitKind, UnitTask};
    let shared = ship_on_water().await;
    let mut client = shared.borrow_mut();
    for (moving, task, pose) in [
        (false, UnitTask::Idle, Pose::FishingShipStand),
        (true, UnitTask::Moving, Pose::FishingShipWalk),
        (false, UnitTask::Fishing, Pose::FishingShipWork),
    ] {
        let ship = client.units.get_mut(&EntityId(1)).unwrap();
        ship.moving = moving;
        ship.task = task;
        assert_eq!(unit_pose(ship, None), pose);
        assert_eq!(
            units(&client, &prepare(&client))[0].kind,
            aoe_rendering::SceneUnitKind::actor(pose as u8)
        );
    }
    let fisher = client.units.get_mut(&EntityId(1)).unwrap();
    fisher.kind = UnitKind::Villager;
    assert_eq!(unit_pose(fisher, None), Pose::FisherWork);
    assert_eq!(
        units(&client, &prepare(&client))[0].kind,
        aoe_rendering::SceneUnitKind::actor(Pose::FisherWork as u8)
    );
    for facing in 0..8 {
        assert_eq!(
            packed_frame(Pose::FisherWork, facing, 399.0, None, 0.0, 20),
            u16::from(facing)
        );
        assert_eq!(
            packed_frame(Pose::FisherWork, facing, 400.0, None, 0.0, 20),
            u16::from(facing) | (1 << 3)
        );
    }
    assert_eq!(
        click_order(UnitKind::FishingShip, false, false, false, true),
        ClickOrder::Gather
    );
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn nearer_ordinary_ground_hides_a_fishing_ship_from_selection() {
    let shared = ship_on_water().await;
    {
        let client = shared.borrow();
        let mut cached = client.terrain_scene.borrow_mut();
        let scene = Rc::get_mut(cached.as_mut().unwrap()).unwrap();
        // A raised ordinary surface two tiles ahead projects over the ship.
        // Its source footprint stays away from the ship's water contact.
        for triangle in scene.triangles.iter_mut().filter(|t| t.tile == [284, 605]) {
            assert!(!triangle.skirt);
            for point in &mut triangle.points {
                point.world[2] = 4.0;
                point.screen = client
                    .camera
                    .world_to_screen_at_height([point.world[0], point.world[1]], point.world[2]);
            }
        }
        assert_eq!(scene.height([282.5, 603.5]), Some(0.0));
        let target = aoe_core::ScreenPoint { x: 256.0, y: 256.0 };
        assert!(aoe_rendering::surface_depth_at(&scene.triangles, target).unwrap() > 890.0);
        assert!(!aoe_rendering::cliff_face_in_front(
            &scene.triangles,
            target,
            887.5
        ));
    }
    click(&shared, [256.0, 256.0]);
    let selected = shared.borrow().selected.units.clone();
    shared.borrow().canvas.remove();
    assert!(selected.is_empty(), "hidden hull must not be selectable");
}

#[wasm_bindgen_test]
fn gather_cursor_names_fishing_never_building() {
    use crate::playground::game::village::presentation::gather_cursor;
    use aoe_map::{ObjectKind, ResourceKind};
    assert_eq!(
        gather_cursor(ObjectKind::ShoreFish, ResourceKind::Food),
        6.0
    );
    assert_eq!(gather_cursor(ObjectKind::DeepFish, ResourceKind::Food), 6.0);
    assert_eq!(
        gather_cursor(ObjectKind::ForageBush, ResourceKind::Food),
        7.0
    );
    assert_eq!(gather_cursor(ObjectKind::Tree, ResourceKind::Wood), 8.0);
    assert_eq!(
        gather_cursor(ObjectKind::GoldDeposit, ResourceKind::Gold),
        9.0
    );
}

#[wasm_bindgen_test]
async fn dead_naval_units_never_render_an_intact_living_hull() {
    use aoe_protocol::{UnitKind, UnitTask};
    let scene = super::contact_scene();
    let mut client = super::client_with_backend(&scene, Some("canvas")).await;
    for kind in [
        UnitKind::FishingShip,
        UnitKind::Galley,
        UnitKind::FireShip,
        UnitKind::DemolitionShip,
        UnitKind::TransportShip,
        UnitKind::WarGalley,
    ] {
        let ship = client.units.get_mut(&EntityId(1)).unwrap();
        ship.kind = kind;
        ship.hp = 50;
        ship.task = UnitTask::Idle;
        ship.death_tick = None;
        assert!(
            units(&client, &scene)
                .iter()
                .any(|unit| unit.id == EntityId(1)),
            "living {kind:?} remains visible"
        );
        let ship = client.units.get_mut(&EntityId(1)).unwrap();
        ship.hp = 0;
        ship.task = UnitTask::Dying;
        ship.death_tick = Some(aoe_core::Tick(0));
        assert!(
            !units(&client, &scene)
                .iter()
                .any(|unit| unit.id == EntityId(1)),
            "dead {kind:?} must not retain its intact hull"
        );
    }
    client.units.get_mut(&EntityId(1)).unwrap().kind = UnitKind::Militia;
    assert!(
        units(&client, &scene)
            .iter()
            .any(|unit| unit.id == EntityId(1)),
        "land units retain their native corpse presentation"
    );
    client.canvas.remove();
}
