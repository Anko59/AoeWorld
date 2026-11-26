//! Orders given at a world point, as a minimap click gives them: there is no
//! screen position to pick with, and the units may be far from the camera.
//! Mounted by the scene picking tests, which own the client fixture.
use super::{building, fixture};
use crate::playground::game::{
    Client, map,
    village::{orders, picking},
};
use aoe_core::{
    EntityId, Formation, MAX_FORMATION_MEMBERS, PlayerId, ScreenPoint, TileCoord, TileRect,
    WorldPosition,
};
use aoe_protocol::{
    BuildingKind, CommandResult, GameplayClientMessage as Message, RallyTarget, UnitKind,
    decode_gameplay_client,
};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::WebSocket;

#[wasm_bindgen(inline_js = "
export function record_frames(socket) {
  socket.frames = [];
  Object.defineProperty(socket, 'bufferedAmount', { configurable: true, value: 0 });
  Object.defineProperty(socket, 'readyState', { configurable: true, value: 1 });
  socket.send = bytes => { socket.frames.push(Uint8Array.from(bytes)); };
}
export function take_frame(socket) { return socket.frames.shift() ?? new Uint8Array(); }
")]
extern "C" {
    fn record_frames(socket: &WebSocket);
    fn take_frame(socket: &WebSocket) -> Vec<u8>;
}

async fn recorded() -> Client {
    let mut client = fixture(1.0).await;
    let socket = WebSocket::new("ws://127.0.0.1:18767").unwrap();
    record_frames(&socket);
    client.socket = Some(socket);
    client
}

/// Every order sent since the last call, as the server decodes it.
fn sent(client: &Client) -> Vec<Message> {
    let socket = client.socket.as_ref().unwrap();
    std::iter::from_fn(|| Some(take_frame(socket)).filter(|bytes| !bytes.is_empty()))
        .map(|bytes| decode_gameplay_client(&bytes).unwrap())
        .collect()
}

fn center(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

/// Another living unit of the fixture's owner.
fn add(client: &mut Client, id: u32, kind: UnitKind, tile: [i32; 2]) -> EntityId {
    let mut state = client.units[&EntityId(1)];
    state.id = EntityId(id);
    state.kind = kind;
    state.position = center(tile[0], tile[1]);
    client.units.insert(state.id, state);
    state.id
}

fn move_order(sequence: u64, entity_id: EntityId, destination: WorldPosition) -> Message {
    Message::MoveOrder {
        sequence,
        entity_id,
        destination,
        attack_move: false,
    }
}

fn berries(client: &mut Client, tile: TileCoord) {
    let node = aoe_map::ResourceNode::starting(tile, aoe_map::ResourceKind::Food).unwrap();
    assert!(client.resources.apply(aoe_protocol::ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: None,
        revision: 0,
        changes: vec![aoe_protocol::ResourceAmount {
            id: node.id,
            remaining: node.initial_amount,
        }],
    }));
}

/// A mode a click can be given in: attack-move, patrol, General alone, queued.
type Mode = (bool, bool, bool, bool);
const MODES: [Mode; 8] = [
    (false, false, false, false),
    (false, false, false, true),
    (true, false, false, false),
    (true, false, false, true),
    (false, true, false, false),
    (false, true, false, true),
    (false, false, true, false),
    (false, false, true, true),
];

fn arm(client: &mut Client, (attack, patrol, alone, _): Mode) {
    client.village.clear_move_cursor();
    client.village.attack_move = attack;
    client.village.patrol_move = patrol;
    client.village.general_only = alone;
    client.village.move_cursor = attack || patrol;
}

fn armed(client: &Client) -> [bool; 4] {
    [
        client.village.move_cursor,
        client.village.attack_move,
        client.village.patrol_move,
        client.village.general_only,
    ]
}

#[wasm_bindgen_test]
async fn a_world_point_orders_exactly_what_a_right_click_on_that_empty_ground_orders() {
    let mut client = recorded().await;
    client.village.formation = Formation::Box;
    for (id, kind) in [
        (2, UnitKind::Knight),
        (3, UnitKind::General),
        (4, UnitKind::Villager),
        (5, UnitKind::Galley),
    ] {
        add(&mut client, id, kind, [96, 104]);
    }
    client.village.animals.push(aoe_protocol::AnimalState {
        id: EntityId(6),
        kind: aoe_protocol::AnimalKind::Sheep,
        owner: Some(PlayerId(1)),
        hp: 7,
        position: center(96, 104),
        task: aoe_protocol::AnimalTask::Idle,
        food_remaining: 100,
        target: None,
        facing: 0,
        moving: false,
        death_tick: None,
    });
    let point = ScreenPoint {
        x: 1100.0,
        y: 240.0,
    };
    let world = map::world_at_screen(&client, point).unwrap();
    assert_eq!(picking::actor(&client, point), None);
    let selections = [
        vec![1, 2, 3, 4, 5, 6],
        vec![1, 2, 3],
        vec![3],
        vec![5],
        vec![6],
    ];
    for (index, selection) in selections.into_iter().enumerate() {
        let selection: Vec<_> = selection.into_iter().map(EntityId).collect();
        for mode in MODES {
            client.selected.units(selection.clone());
            arm(&mut client, mode);
            client.next_sequence = 1;
            orders::at_pointer(&mut client, point, mode.3);
            let (expected, left_armed) = (sent(&client), armed(&client));
            assert!(index > 0 || !expected.is_empty(), "{mode:?} orders an army");
            arm(&mut client, mode);
            client.next_sequence = 1;
            orders::at_world(&mut client, world, mode.3);
            assert_eq!(sent(&client), expected, "{selection:?} {mode:?}");
            assert_eq!(armed(&client), left_armed, "{selection:?} {mode:?}");
        }
    }
    client.canvas.remove();
}

/// The server streams every unit the viewer owns, whatever the subscribed
/// region; only the order reaching it proves the client keeps and uses them.
#[wasm_bindgen_test]
async fn owned_units_far_outside_the_view_are_ordered_and_the_camera_stays() {
    let mut client = recorded().await;
    client.sent_region = Some(TileRect::from_xywh(90, 90, 20, 20));
    let near = add(&mut client, 2, UnitKind::Knight, [9000, 12000]);
    let far = add(&mut client, 3, UnitKind::Knight, [15000, 40]);
    let dead = add(&mut client, 4, UnitKind::Knight, [9000, 12000]);
    client.units.get_mut(&dead).unwrap().hp = 0;
    let foreign = add(&mut client, 5, UnitKind::Knight, [101, 101]);
    client.units.get_mut(&foreign).unwrap().player = PlayerId(2);
    for (sequence, zoom) in [(1, 1.0), (2, 1.0 / 512.0)] {
        client.camera.zoom = zoom;
        let camera = client.camera;
        client.selected.units(vec![far, dead, foreign, near]);
        orders::at_world(&mut client, [4000.3, 6000.7], false);
        assert_eq!(
            sent(&client),
            [Message::FormationOrder {
                sequence,
                unit_ids: vec![near, far],
                formation: client.village.formation,
                destination: Some(center(4000, 6000)),
                queued: false,
                attack_move: false,
            }]
        );
        assert_eq!(client.camera, camera);
    }
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn a_selection_over_the_formation_limit_is_sent_in_chunks() {
    let mut client = recorded().await;
    let units: Vec<_> = (0..=MAX_FORMATION_MEMBERS as u32)
        .map(|index| add(&mut client, 10 + index, UnitKind::Knight, [9000, 9000]))
        .collect();
    client.selected.units(units.clone());
    orders::at_world(&mut client, [4000.5, 6000.5], false);
    let chunks = sent(&client);
    assert_eq!(chunks.len(), 2);
    let Message::FormationOrder { unit_ids, .. } = &chunks[0] else {
        panic!("the first chunk keeps its formation");
    };
    assert_eq!(unit_ids[..], units[..MAX_FORMATION_MEMBERS]);
    assert_eq!(
        chunks[1],
        move_order(2, units[MAX_FORMATION_MEMBERS], center(4000, 6000))
    );
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn nothing_is_sent_without_an_owned_selection_or_outside_the_world() {
    let mut client = recorded().await;
    let foreign = add(&mut client, 2, UnitKind::Knight, [101, 101]);
    client.units.get_mut(&foreign).unwrap().player = PlayerId(2);
    let mut house = building(9, 5000, 5000);
    house.kind = BuildingKind::House;
    let mut enemy = building(10, 5100, 5000);
    enemy.player = PlayerId(2);
    client.village.buildings.extend([house, enemy]);
    for units in [vec![], vec![foreign]] {
        client.selected.units(units);
        for mode in MODES {
            arm(&mut client, mode);
            orders::at_world(&mut client, [4000.5, 6000.5], mode.3);
        }
    }
    arm(&mut client, MODES[0]);
    for id in [9, 10] {
        client.selected.building(EntityId(id));
        orders::at_world(&mut client, [4000.5, 6000.5], false);
    }
    client.selected.units(vec![EntityId(1)]);
    let (width, height) = (client.config.width_tiles, client.config.height_tiles);
    let (right, bottom) = (f64::from(width), f64::from(height));
    for world in [
        [-0.01, 5.0],
        [5.0, -3.0],
        [right, 5.0],
        [5.0, bottom],
        [f64::NAN, 5.0],
        [5.0, f64::NAN],
        [f64::INFINITY, 5.0],
    ] {
        orders::at_world(&mut client, world, false);
    }
    assert!(sent(&client).is_empty());
    assert_eq!(client.next_sequence, 1);
    orders::at_world(&mut client, [right - 0.01, bottom - 0.01], false);
    assert_eq!(
        sent(&client),
        [move_order(1, EntityId(1), center(width - 1, height - 1))]
    );
    client.canvas.remove();
}

/// One minimap pixel covers many tiles, so even a point on an own building, an
/// animal or a resource rallies on the ground.
#[wasm_bindgen_test]
async fn selected_production_buildings_take_a_ground_rally_at_the_point() {
    let mut client = recorded().await;
    berries(&mut client, TileCoord::new(100, 100));
    let mut barracks = building(9, 5000, 5000);
    barracks.kind = BuildingKind::Barracks;
    let mut house = building(10, 5100, 5000);
    house.kind = BuildingKind::House;
    let mut enemy = building(11, 5200, 5000);
    enemy.player = PlayerId(2);
    client.village.buildings.extend([barracks, house, enemy]);
    client.selected.building(EntityId(8));
    client.selected.buildings = [8, 9, 10, 11].map(EntityId).to_vec();
    let camera = client.camera;
    // The Town Center with id 8 covers this tile itself.
    orders::at_world(&mut client, [100.4, 100.6], false);
    let rally = |sequence, building_id| Message::SetRallyOrder {
        sequence,
        building_id: EntityId(building_id),
        target: RallyTarget::Ground(center(100, 100)),
    };
    assert_eq!(sent(&client), [rally(1, 8), rally(2, 9)]);
    assert_eq!(client.camera, camera);
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn a_world_point_never_attacks_gathers_or_repairs_what_stands_on_it() {
    let mut client = recorded().await;
    client.village.buildings.clear();
    let villager = add(&mut client, 2, UnitKind::Villager, [110, 110]);
    client.primary = Some(villager);
    client.selected.units(vec![villager]);
    client.units.get_mut(&EntityId(1)).unwrap().player = PlayerId(2);
    client.fog.apply(aoe_protocol::FogState {
        revision: 1,
        tick: aoe_core::Tick(0),
        region: TileRect::from_xywh(0, 0, 1, 1),
        tiles: vec![],
        overview: vec![],
        disabled: true,
    });
    let point = ScreenPoint { x: 640.0, y: 270.0 };
    let world = map::world_at_screen(&client, point).unwrap();
    let ground = center(98, 98);

    // A right-click on the screen here would attack this enemy scout.
    let enemy = Some((EntityId(1), false, true));
    assert_eq!(picking::actor(&client, point), enemy);
    orders::at_world(&mut client, world, false);
    assert_eq!(sent(&client), [move_order(1, villager, ground)]);

    client.units.remove(&EntityId(1));
    berries(&mut client, TileCoord::new(98, 98));
    assert!(picking::resource(&client, point).is_some());
    orders::at_world(&mut client, world, false);
    assert_eq!(sent(&client), [move_order(2, villager, ground)]);

    client.resources.clear();
    let mut damaged = building(8, 97, 97);
    damaged.hp = 100;
    client.village.buildings.push(damaged);
    orders::at_world(&mut client, world, false);
    assert_eq!(sent(&client), [move_order(3, villager, ground)]);
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn a_refused_point_names_only_an_obstacle_the_client_knows() {
    let mut client = recorded().await;
    berries(&mut client, TileCoord::new(60, 60));
    orders::at_world(&mut client, [60.5, 60.5], false);
    orders::at_world(&mut client, [9000.5, 9000.5], false);
    let refused = CommandResult::RejectedInvalidDestination;
    assert_eq!(
        client.order_feedback.acknowledge(1, refused),
        "Cannot move there: a forage bush is in the way."
    );
    assert_eq!(
        client.order_feedback.acknowledge(2, refused),
        "Cannot move there: the destination is blocked or outside the map."
    );
    client.canvas.remove();
}

#[path = "minimap/presses.rs"]
mod presses;
