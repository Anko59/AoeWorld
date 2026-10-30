use super::*;
use aoe_core::{EntityId, PlayerId, WorldConfig, WorldPosition};
use aoe_protocol::GameplayUnitState;
use aoe_rendering::{GameArt, GameRenderer};
use std::collections::BTreeMap;
use wasm_bindgen_test::wasm_bindgen_test;

#[path = "budget.rs"]
mod budget;
#[path = "fishing.rs"]
mod fishing;
#[path = "fixture.rs"]
mod fixture;
#[path = "gates.rs"]
mod gates;
#[path = "picking.rs"]
mod picking;

fn contact_scene() -> PreparedScene {
    let generator = fixture::plane(-432, -432);
    let source = generator
        .landscape_point_with_cancel(aoe_core::TileCoord::new(282, 603), &|| false)
        .unwrap()
        .unwrap()
        .tile
        .terrain;
    assert!(source.passable);
    assert_eq!(source.surface.corner_game_height_levels, [79, 79, 78, 79]);
    let terrain = fixture::region(&generator, 270..295, 590..615);
    for position in [[281.5, 602.5], [282.5, 602.5], [281.5, 603.5]] {
        let ledge = terrain.iter().find(|s| s.position == position).unwrap();
        assert_eq!(ledge.surface.kind, SceneTerrainSurface::CLIFF_LEDGE);
        assert_eq!(ledge.surface.corner_game_height_levels, [80; 4]);
    }
    PreparedScene::new(
        Camera {
            center: [282.5, 603.5],
            zoom: 1.0,
            viewport: [512.0; 2],
            focus_elevation_meters: 79.0,
        },
        terrain,
    )
}

#[wasm_bindgen_test]
fn review_f1_passable_center_contacts_source_ground_below_the_cliff() {
    let scene = contact_scene();
    assert_eq!(scene.height([282.5, 603.5]), Some(78.5));
}

#[wasm_bindgen_test]
fn review_f1_rim_cannot_cover_the_neighboring_passable_center() {
    let scene = contact_scene();
    let cliff: Vec<_> = scene
        .triangles
        .iter()
        .filter(|t| !t.skirt && t.tile == [281, 602])
        .collect();
    assert_eq!(cliff.len(), 2);
    let corner = cliff
        .iter()
        .flat_map(|t| t.points)
        .find(|p| p.world[0] > 281.9 && p.world[1] > 602.9)
        .unwrap();
    for axis in 0..2 {
        assert!(
            (corner.world[axis] - [282.0, 603.0][axis]).abs() < 0.5,
            "rim crosses tile center: {:?}",
            corner.world
        );
    }
    // Every point of the central walkable square stays outside the ledge tops.
    for x in [282.3, 282.5, 282.7] {
        for y in [603.3, 603.5, 603.7] {
            assert!(scene.height([x, y]).unwrap() < 80.0);
        }
    }
}

#[wasm_bindgen_test]
async fn review_f1_unit_dto_stays_on_source_ground_instead_of_cliff_top() {
    let scene = contact_scene();
    let client = client(&scene).await;
    let units = units(&client, &scene);
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].position, [282.5, 603.5]);
    assert_eq!(units[0].elevation_meters, 78.5);
    assert_eq!(
        client
            .camera
            .world_to_screen_at_height(units[0].position, units[0].elevation_meters),
        client
            .camera
            .world_to_screen_at_height([282.5, 603.5], 78.5)
    );
}

#[wasm_bindgen_test]
async fn nonzero_viewer_keeps_transport_control_when_the_primary_scout_is_aboard() {
    use crate::playground::game::village::owner;
    let scene = contact_scene();
    let mut client = client_with_backend(&scene, Some("canvas")).await;
    let scout = client.primary.unwrap();
    client.units.get_mut(&scout).unwrap().player = PlayerId(7);
    assert_eq!(owner(&client), PlayerId(7));
    let mut transport = client.units.remove(&scout).unwrap();
    transport.id = EntityId(99);
    transport.kind = aoe_protocol::UnitKind::TransportShip;
    transport.cargo_count = 1;
    client.units.insert(transport.id, transport);
    assert!(!client.units.contains_key(&scout));
    assert_eq!(owner(&client), PlayerId(7));
    assert_eq!(client.units[&transport.id].player, owner(&client));
    assert_ne!(owner(&client), PlayerId(0));
    client.canvas.remove();
}

pub(super) async fn client(scene: &PreparedScene) -> Client {
    client_with_backend(scene, None).await
}

pub(super) async fn client_with_backend(scene: &PreparedScene, backend: Option<&str>) -> Client {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    if backend == Some("canvas") {
        document.body().unwrap().append_child(&canvas).unwrap();
    }
    let (renderer, canvas) = GameRenderer::new_with_backend(canvas, backend)
        .await
        .unwrap();
    let id = EntityId(1);
    Client {
        native: None,
        hover_mode: 1,
        team_colour: [65, 145, 245],
        document,
        canvas,
        renderer,
        art: GameArt {
            resident: Vec::new(),
            actors: Vec::new(),
            actor_rows: Vec::new(),
            buildings: std::array::from_fn(|_| Vec::new()),
            village: Default::default(),
            fishing: Default::default(),
            compositions: Vec::new(),
            foundation: Default::default(),
            walking: Vec::new(),
            standing: Vec::new(),
            grass: Vec::new(),
            terrain: Default::default(),
            terrain_topology: [None; 12],
            water_depth: Default::default(),
            resources: Default::default(),
            tree_shadows: Vec::new(),
            tree_families: Default::default(),
            scenery: Default::default(),
        },
        socket: None,
        connection_callbacks: None,
        connection_id: 0,
        camera: scene.camera,
        capture: Default::default(),
        config: WorldConfig::default(),
        primary: Some(id),
        map_content_hash: None,
        surface_fixture: false,
        focus_map_hash: None,
        resources: Default::default(),
        fog: Default::default(),
        terrain_scene: RefCell::new(None),
        rendered_frame: Default::default(),
        terrain_chunks: BTreeMap::new(),
        terrain_height_bounds: None,
        terrain_bounds: Default::default(),
        terrain_resident_height_bounds: None,
        terrain_requests: Default::default(),
        token: None,
        revision: 0,
        sent_region: None,
        last_subscribe: 0.0,
        next_sequence: 1,
        order_feedback: Default::default(),
        units: BTreeMap::from([(
            id,
            GameplayUnitState {
                id,
                player: PlayerId(1),
                position: WorldPosition::from_tile_center(aoe_core::TileCoord::new(282, 603))
                    .unwrap(),
                moving: false,
                planning: false,
                facing: 0,
                kind: aoe_protocol::UnitKind::Scout,
                task: aoe_protocol::UnitTask::Idle,
                carried: None,
                gathering: None,
                hp: 25,
                max_hp: 25,
                attack_target: None,
                last_hit: None,
                death_tick: None,
                hunt_target: None,
                death_hunt: None,
                auto_scout: false,
                cargo_count: 0,
                home_castle: None,
                managed_by: None,
                commanded_by: None,
                general: None,
                stance: Default::default(),
            },
        )]),
        presentation: Default::default(),
        selected: crate::playground::game::village::selection::Selection {
            units: vec![id],
            building_id: None,
            resource_id: None,
            buildings: Vec::new(),
        },
        village: Default::default(),
        grid: false,
        drag: None,
        pointer: None,
        last_frame: 0.0,
        status: String::new(),
        last_error: None,
    }
}

#[wasm_bindgen_test]
async fn successive_unit_deltas_cannot_grow_the_resident_table_beyond_its_budget() {
    let scene = PreparedScene::new(
        Camera {
            center: [100.0, 100.0],
            zoom: 1.0,
            viewport: [640.0, 360.0],
            focus_elevation_meters: 0.0,
        },
        vec![],
    );
    let mut client = client(&scene).await;
    let mut state = *client.units.values().next().unwrap();
    client.units.clear();
    for id in 0..aoe_protocol::MAX_SUBSCRIBED_UNITS {
        state.id = EntityId(id as u32);
        crate::playground::game::remember(&mut client, &state, 1);
    }
    state.id = EntityId(aoe_protocol::MAX_SUBSCRIBED_UNITS as u32);
    assert!(!crate::playground::game::remember(&mut client, &state, 1));
    assert_eq!(client.units.len(), aoe_protocol::MAX_SUBSCRIBED_UNITS);
    assert!(!client.units.contains_key(&state.id));
    state.id = EntityId(0);
    state.facing = 3;
    crate::playground::game::remember(&mut client, &state, 2);
    assert_eq!(client.units[&state.id].facing, 3);
    client.units.remove(&state.id);
    client.presentation.remove(state.id);
    state.id = EntityId(aoe_protocol::MAX_SUBSCRIBED_UNITS as u32);
    crate::playground::game::remember(&mut client, &state, 3);
    assert!(client.units.contains_key(&state.id));
    assert_eq!(client.units.len(), aoe_protocol::MAX_SUBSCRIBED_UNITS);
}

#[wasm_bindgen_test]
async fn a_lumberjack_keeps_chopping_with_a_partial_load_until_returning() {
    use aoe_protocol::{CarriedResource, ResourceKind, UnitKind, UnitTask};
    let scene = contact_scene();
    let mut client = client(&scene).await;
    let id = EntityId(1);
    let worker = client.units.get_mut(&id).unwrap();
    worker.kind = UnitKind::Villager;
    worker.task = UnitTask::Gathering;
    worker.carried = Some(CarriedResource {
        kind: ResourceKind::Wood,
        amount: 1,
    });
    client.units.get_mut(&id).unwrap().gathering = Some(ResourceKind::Wood);
    assert_eq!(
        units(&client, &scene)[0].kind,
        aoe_rendering::SceneUnitKind::VillagerChopping
    );
    client.units.get_mut(&id).unwrap().task = UnitTask::Returning;
    assert_eq!(
        units(&client, &scene)[0].kind,
        aoe_rendering::SceneUnitKind::VillagerCarryingWood
    );
}

#[wasm_bindgen_test]
async fn every_authoritative_resource_task_has_work_and_carry_art() {
    use aoe_protocol::{CarriedResource, ResourceKind, UnitKind, UnitTask};
    let scene = contact_scene();
    let mut client = client(&scene).await;
    let id = EntityId(1);
    for kind in [
        ResourceKind::Food,
        ResourceKind::Wood,
        ResourceKind::Gold,
        ResourceKind::Stone,
    ] {
        let worker = client.units.get_mut(&id).unwrap();
        worker.kind = UnitKind::Villager;
        worker.task = UnitTask::Gathering;
        worker.gathering = Some(kind);
        worker.carried = Some(CarriedResource { kind, amount: 1 });
        let working = units(&client, &scene)[0].kind;
        assert_ne!(
            working,
            aoe_rendering::SceneUnitKind::Villager,
            "work: {kind:?}"
        );
        client.units.get_mut(&id).unwrap().task = UnitTask::Returning;
        let carrying = units(&client, &scene)[0].kind;
        assert_ne!(
            carrying,
            aoe_rendering::SceneUnitKind::Villager,
            "carry: {kind:?}"
        );
        assert_ne!(
            working, carrying,
            "partial load must keep working: {kind:?}"
        );
        client.units.get_mut(&id).unwrap().task = UnitTask::Idle;
        assert_eq!(
            units(&client, &scene)[0].kind,
            aoe_rendering::SceneUnitKind::Villager
        );
    }
}

#[wasm_bindgen_test]
async fn remembered_building_art_uses_last_seen_age_instead_of_live_owner_age() {
    use aoe_core::{TileCoord, TileRect};
    use aoe_protocol::{
        Age, BuildingKind, ConstructionState, GameplayBuildingState, GameplayPlayerState,
        MarketPrices, Stockpile, Tech,
    };
    let scene = contact_scene();
    let mut client = client(&scene).await;
    client.village.players.push(GameplayPlayerState {
        player: PlayerId(2),
        stockpile: Stockpile::default(),
        population_used: 0,
        population_cap: 5,
        age: Age::Feudal,
        researched: Tech::FeudalAge.bit(),
        market_prices: MarketPrices::default(),
    });
    client.village.buildings.push(GameplayBuildingState {
        id: EntityId(2),
        player: PlayerId(2),
        kind: BuildingKind::House,
        footprint: TileRect::new(TileCoord::new(282, 603), TileCoord::new(284, 605)),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 550,
        max_hp: 550,
        los: 4,
        food_remaining: None,
        training_queue: vec![],
        research: None,
        last_hit: None,
        age: Age::Dark,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
        bridge_span: None,
    });
    let rendered = |client: &Client| {
        units(client, &scene)
            .into_iter()
            .find(|u| u.id == EntityId(2))
            .unwrap()
            .kind
    };
    assert_eq!(rendered(&client), aoe_rendering::SceneUnitKind::House);
    client.village.buildings[0].age = Age::Feudal;
    assert_eq!(rendered(&client), aoe_rendering::SceneUnitKind::FeudalHouse);
}

#[wasm_bindgen_test]
async fn authoritative_gate_stays_open_when_the_holding_enemy_is_not_resident() {
    use aoe_core::{TileCoord, TileRect};
    use aoe_protocol::{Age, BuildingKind, ConstructionState, GameplayBuildingState};
    let scene = contact_scene();
    let mut client = client(&scene).await;
    client.units.clear(); // The unit keeping this gate open has left our sight.
    client.village.buildings.push(GameplayBuildingState {
        id: EntityId(2),
        player: PlayerId(1),
        kind: BuildingKind::PalisadeGate,
        footprint: TileRect::new(TileCoord::new(281, 603), TileCoord::new(285, 604)),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 400,
        max_hp: 400,
        los: 6,
        food_remaining: None,
        training_queue: vec![],
        research: None,
        last_hit: None,
        age: Age::Feudal,
        gate_open: true,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
        bridge_span: None,
    });
    for _ in 0..20 {
        let gate = units(&client, &scene)
            .into_iter()
            .find(|u| u.id == EntityId(2))
            .unwrap();
        assert_eq!(gate.kind, aoe_rendering::SceneUnitKind::OpenPalisadeGate);
    }
    assert_eq!(client.village.gate_art.borrow().updates(), 1);
    client.village.buildings[0].gate_open = false;
    assert_eq!(
        units(&client, &scene)[0].kind,
        aoe_rendering::SceneUnitKind::PalisadeGate
    );
    assert_eq!(client.village.gate_art.borrow().updates(), 2);
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn projectile_scene_uses_native_arrow_and_javelin_and_current_fog() {
    use aoe_core::{Tick, TileCoord, TileRect};
    use aoe_protocol::{FogState, ProjectileKind, ProjectileState};
    let scene = contact_scene();
    let mut client = client(&scene).await;
    client.units.clear();
    client.village.tick = 15;
    let region = TileRect::new(TileCoord::new(281, 602), TileCoord::new(286, 606));
    let fog = |state| FogState {
        revision: 1,
        tick: Tick(15),
        region,
        disabled: false,
        tiles: vec![state; 20],
        overview: vec![],
    };
    assert!(client.fog.apply(fog(2)));
    for kind in [ProjectileKind::Arrow, ProjectileKind::Javelin] {
        client.village.projectiles = vec![ProjectileState {
            id: 1,
            attacker: None,
            from: WorldPosition::from_tile_center(TileCoord::new(282, 603)).unwrap(),
            to: WorldPosition::from_tile_center(TileCoord::new(284, 603)).unwrap(),
            launched: Tick(10),
            impact: Tick(20),
            kind,
        }];
        let rendered = units(&client, &scene);
        assert_eq!(rendered.len(), 1);
        assert_eq!(rendered[0].position, [283.5, 603.5]);
        assert!(
            rendered[0].elevation_meters > scene.height(rendered[0].position).unwrap() + 1.0,
            "mid-flight projectile arcs above its launch height"
        );
        assert_eq!(
            rendered[0].kind,
            if kind == ProjectileKind::Arrow {
                aoe_rendering::SceneUnitKind::Arrow
            } else {
                aoe_rendering::SceneUnitKind::Javelin
            }
        );
    }
    assert!(client.fog.apply(fog(1)));
    assert!(units(&client, &scene).is_empty());
    client.canvas.remove();
}
