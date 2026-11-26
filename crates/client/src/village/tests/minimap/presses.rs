//! What a press on the map or the canvas sends and leaves armed, through the
//! client's own listeners, and the orders both kinds of press share.
use super::*;
use crate::playground::game::{controls, village::construction};
use std::{cell::RefCell, rc::Rc};
use web_sys::HtmlCanvasElement;

#[wasm_bindgen(inline_js = "
let send, navigation;
export function bind_presses() {
  navigation = window.aoeNavigation;
  window.aoeNavigation = { bindOrders(callback) { send = callback; } };
}
export function end_binding() { window.aoeNavigation = navigation; }
export function map_press(x, y, left, queued, alt) { return send(x, y, left, queued, alt); }
export function pointer(canvas, type, x, y, button, altKey) {
  const box = canvas.getBoundingClientRect();
  canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, pointerId: 1,
    button, altKey, clientX: box.left + x, clientY: box.top + y }));
}
")]
extern "C" {
    fn bind_presses();
    fn end_binding();
    /// Zero when the press is no order: the map then drags or jumps.
    fn map_press(x: f64, y: f64, left: bool, queued: bool, alt: bool) -> u32;
    fn pointer(canvas: &HtmlCanvasElement, kind: &str, x: f64, y: f64, button: i32, alt: bool);
}

async fn bound() -> Rc<RefCell<Client>> {
    let client = recorded().await;
    let style = "position:fixed;left:0;top:0;width:1280px;height:720px";
    client.canvas.set_attribute("style", style).unwrap();
    let shared = Rc::new(RefCell::new(client));
    bind_presses();
    controls::install(shared.clone()).unwrap();
    end_binding();
    shared
}

/// Arms a destination the way its command button does.
fn show(client: &mut Client, mode: Mode, name: &str) {
    arm(client, mode);
    client.village.move_cursor = true;
    client
        .canvas
        .set_attribute("data-order-mode", name)
        .unwrap();
}

/// Nothing is armed and the cursor on the screen says so.
fn assert_disarmed(client: &Client, case: &str) {
    assert_eq!(armed(client), [false; 4], "{case}");
    let shown = client.canvas.get_attribute("data-order-mode");
    assert_eq!(shown.unwrap_or_default(), "", "{case}");
}

const LEFT: bool = true;
const RIGHT: bool = false;
const AT: [f64; 2] = [4000.5, 6000.5];

fn at(button: bool, alt: bool) -> u32 {
    map_press(AT[0], AT[1], button, false, alt)
}

#[wasm_bindgen_test]
async fn cancelling_a_placement_leaves_no_order_armed_behind_a_plain_cursor() {
    let shared = bound().await;
    let canvas = shared.borrow().canvas.clone();
    add(&mut shared.borrow_mut(), 2, UnitKind::Knight, [110, 110]);
    for case in ["command", "pointercancel", "canvas", "map", "map border"] {
        {
            let mut client = shared.borrow_mut();
            client.selected.units(vec![EntityId(2)]);
            client.village.placement = Some(BuildingKind::House);
            show(&mut client, MODES[0], "move");
        }
        match case {
            "command" => construction::cancel(&mut shared.borrow_mut()),
            "pointercancel" => pointer(&canvas, case, 1100.0, 240.0, 0, false),
            "canvas" => pointer(&canvas, "pointerdown", 1100.0, 240.0, 2, false),
            "map" => {
                at(RIGHT, false);
            }
            _ => {
                map_press(f64::NAN, f64::NAN, RIGHT, false, false);
            }
        }
        assert_eq!(shared.borrow().village.placement, None, "{case}");
        assert_disarmed(&shared.borrow(), case);
        assert_eq!(at(LEFT, false), 0, "{case}: the map jumps");
        assert!(sent(&shared.borrow()).is_empty(), "{case}");
    }
    // The next left click on the canvas selects the scout on the ground there.
    show(&mut shared.borrow_mut(), MODES[0], "move");
    for kind in ["pointercancel", "pointerdown", "pointerup"] {
        pointer(&canvas, kind, 640.0, 270.0, 0, false);
    }
    assert_eq!(shared.borrow().selected.units, [EntityId(1)]);
    assert!(sent(&shared.borrow()).is_empty());
    canvas.remove();
}

#[wasm_bindgen_test]
async fn a_press_with_only_a_building_selected_disarms_and_rallies_on_the_right_button() {
    let shared = bound().await;
    let canvas = shared.borrow().canvas.clone();
    shared.borrow_mut().selected.building(EntityId(8));
    let rally = |sequence, ground| Message::SetRallyOrder {
        sequence,
        building_id: EntityId(8),
        target: RallyTarget::Ground(ground),
    };
    show(&mut shared.borrow_mut(), MODES[2], "attack-move");
    assert_eq!(at(LEFT, false), 0, "the map jumps");
    assert!(sent(&shared.borrow()).is_empty());
    assert_disarmed(&shared.borrow(), "left");

    show(&mut shared.borrow_mut(), MODES[2], "attack-move");
    at(RIGHT, false);
    assert_eq!(sent(&shared.borrow()), [rally(1, center(4000, 6000))]);
    assert_disarmed(&shared.borrow(), "right");

    // Alt names a General; the rally takes the press, so the next order must
    // not walk one alone.
    at(RIGHT, true);
    assert_eq!(sent(&shared.borrow()), [rally(2, center(4000, 6000))]);
    assert_disarmed(&shared.borrow(), "Alt on the map");
    pointer(&canvas, "pointerdown", 1100.0, 240.0, 2, true);
    assert_eq!(sent(&shared.borrow()), [rally(3, center(101, 94))]);
    assert_eq!(armed(&shared.borrow()), [false; 4], "Alt on the canvas");
    canvas.remove();
}

#[wasm_bindgen_test]
async fn an_armed_left_press_orders_only_owned_units_inside_the_world() {
    let shared = bound().await;
    let canvas = shared.borrow().canvas.clone();
    let ground = center(4000, 6000);
    let (knight, general, foreign, sheep) = {
        let mut client = shared.borrow_mut();
        let knight = add(&mut client, 2, UnitKind::Knight, [9000, 9000]);
        let general = add(&mut client, 3, UnitKind::General, [9000, 9000]);
        let foreign = add(&mut client, 4, UnitKind::Knight, [101, 101]);
        client.units.get_mut(&foreign).unwrap().player = PlayerId(2);
        let sheep = animal(&mut client, 6, aoe_protocol::AnimalKind::Sheep, 1);
        (knight, general, foreign, sheep)
    };
    let select = |units: Vec<EntityId>, mode, name| {
        let mut client = shared.borrow_mut();
        client.selected.units(units);
        show(&mut client, mode, name);
    };
    select(vec![knight], MODES[2], "attack-move");
    assert_ne!(map_press(f64::NAN, f64::NAN, LEFT, false, false), 0);
    assert!(sent(&shared.borrow()).is_empty());

    select(vec![foreign], MODES[2], "attack-move");
    assert_eq!(at(LEFT, false), 0, "no owned unit: the map jumps");
    assert!(sent(&shared.borrow()).is_empty());
    assert_disarmed(&shared.borrow(), "foreign");

    // The button of a General alone, not Alt, armed this press.
    select(vec![knight, general], MODES[6], "general-move");
    assert_ne!(at(LEFT, false), 0);
    assert_eq!(
        sent(&shared.borrow()),
        [Message::GeneralOrder {
            sequence: 1,
            entity_id: general,
            command: aoe_protocol::GeneralCommand::MoveAlone {
                destination: ground,
            },
        }]
    );
    assert_disarmed(&shared.borrow(), "General alone");

    // An owned sheep can be moved too, though it is no unit of the army.
    select(vec![sheep], MODES[0], "move");
    assert_ne!(at(LEFT, false), 0);
    assert_eq!(sent(&shared.borrow()), [move_order(2, sheep, ground)]);
    canvas.remove();
}

#[wasm_bindgen_test]
async fn a_world_order_ends_the_other_cursors_and_reports_only_what_was_sent() {
    let mut client = recorded().await;
    let general = add(&mut client, 3, UnitKind::General, [9000, 9000]);
    client.selected.units(vec![EntityId(1), general]);
    for mode in MODES {
        arm(&mut client, mode);
        client.village.garrison_cursor = true;
        client.village.unload_cursor = true;
        assert!(orders::at_world(&mut client, AT, mode.3), "{mode:?}");
        assert!(!client.village.unload_cursor, "{mode:?}");
        // A queued press keeps its mode, so only a plain one ends it.
        assert!(mode.3 || !client.village.garrison_cursor, "{mode:?}");
    }
    let socket = client.socket.take();
    for mode in MODES {
        arm(&mut client, mode);
        assert!(!orders::at_world(&mut client, AT, mode.3), "{mode:?}");
    }
    client.socket = socket;
    client.selected.building(EntityId(8));
    assert!(
        !orders::at_world(&mut client, AT, false),
        "a rally is no ping"
    );
    client.canvas.remove();
}

/// An animal of `owner`, or wild for owner zero, on the fixture's scout tile.
fn animal(client: &mut Client, id: u32, kind: aoe_protocol::AnimalKind, owner: u16) -> EntityId {
    client.village.animals.push(aoe_protocol::AnimalState {
        id: EntityId(id),
        kind,
        owner: (owner > 0).then_some(PlayerId(owner)),
        hp: 5,
        position: center(98, 98),
        task: aoe_protocol::AnimalTask::Idle,
        food_remaining: 100,
        target: None,
        facing: 0,
        moving: false,
        death_tick: None,
    });
    EntityId(id)
}

/// Literal messages: `at_pointer` is the oracle of the other order tests.
#[wasm_bindgen_test]
async fn a_right_click_still_attacks_hunts_gathers_and_queues_with_these_exact_messages() {
    let mut client = recorded().await;
    client.village.buildings.clear();
    let villager = add(&mut client, 2, UnitKind::Villager, [110, 110]);
    let knight = add(&mut client, 3, UnitKind::Knight, [110, 110]);
    client.primary = Some(villager);
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
    let body = ScreenPoint { x: 640.0, y: 250.0 };
    let both = vec![villager, knight];
    client.selected.units(both.clone());

    orders::at_pointer(&mut client, point, false);
    assert_eq!(
        sent(&client),
        [Message::AttackSelectionOrder {
            sequence: 1,
            entity_ids: both.clone(),
            target_id: EntityId(1),
        }]
    );

    client.units.remove(&EntityId(1));
    let deer = animal(&mut client, 7, aoe_protocol::AnimalKind::Deer, 0);
    orders::at_pointer(&mut client, body, false);
    assert_eq!(
        sent(&client),
        [
            Message::HuntOrder {
                sequence: 2,
                unit_ids: vec![villager],
                animal_id: deer,
            },
            Message::AttackOrder {
                sequence: 3,
                entity_id: knight,
                target_id: deer,
            },
        ]
    );

    client.village.animals.clear();
    let tile = TileCoord::new(98, 98);
    berries(&mut client, tile);
    let food = aoe_map::ResourceNode::starting(tile, aoe_map::ResourceKind::Food).unwrap();
    orders::at_pointer(&mut client, point, false);
    assert_eq!(
        sent(&client),
        [
            Message::GatherSelectionOrder {
                sequence: 4,
                entity_ids: vec![villager],
                resource_id: food.id,
            },
            move_order(5, knight, center(98, 98)),
        ]
    );

    client.resources.clear();
    let mut enemy = building(8, 98, 98);
    enemy.player = PlayerId(2);
    client.village.buildings.push(enemy);
    client.selected.units(vec![knight]);
    orders::at_pointer(&mut client, ScreenPoint { x: 640.0, y: 360.0 }, false);
    assert_eq!(
        sent(&client),
        [Message::AttackOrder {
            sequence: 6,
            entity_id: knight,
            target_id: EntityId(8),
        }]
    );

    let empty = ScreenPoint {
        x: 1100.0,
        y: 240.0,
    };
    orders::at_pointer(&mut client, empty, true);
    client.selected.units(both.clone());
    orders::at_pointer(&mut client, empty, true);
    assert_eq!(
        sent(&client),
        [
            Message::QueueMoveOrder {
                sequence: 7,
                entity_id: knight,
                destination: center(101, 94),
                attack_move: false,
            },
            Message::MoveSelectionOrder {
                sequence: 8,
                unit_ids: both,
                formation: client.village.formation,
                destination: center(101, 94),
                queued: true,
                attack_move: false,
            },
        ]
    );
    client.canvas.remove();
}
