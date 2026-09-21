use super::{
    Client, Drag, center_on_primary, dpr, map, position_at, reconnect, send_order, subscribe,
};
use aoe_core::ScreenPoint;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{Event, KeyboardEvent, PointerEvent, WheelEvent};

fn point(client: &Client, event: &PointerEvent) -> ScreenPoint {
    let bounds = client.canvas.get_bounding_client_rect();
    ScreenPoint {
        x: (f64::from(event.client_x()) - bounds.left()) * dpr(),
        y: (f64::from(event.client_y()) - bounds.top()) * dpr(),
    }
}

fn pick(client: &Client, target: ScreenPoint) -> Option<aoe_core::EntityId> {
    client
        .units
        .keys()
        .rev()
        .find(|id| {
            let screen = map::screen_position(client, position_at(client, **id));
            (screen.x - target.x).abs() < 32.0 * client.camera.zoom
                && (screen.y - target.y).abs() < 48.0 * client.camera.zoom
        })
        .copied()
}

fn pick_box(client: &Client, start: ScreenPoint, end: ScreenPoint) -> Option<aoe_core::EntityId> {
    let (min_x, max_x) = (start.x.min(end.x), start.x.max(end.x));
    let (min_y, max_y) = (start.y.min(end.y), start.y.max(end.y));
    client
        .units
        .keys()
        .find(|id| {
            let screen = map::screen_position(client, position_at(client, **id));
            screen.x >= min_x && screen.x <= max_x && screen.y >= min_y && screen.y <= max_y
        })
        .copied()
}

fn pan(client: &mut Client, delta: [f64; 2]) {
    let center = ScreenPoint {
        x: client.camera.viewport[0] / 2.0,
        y: client.camera.viewport[1] / 2.0,
    };
    let before = client
        .camera
        .screen_to_world_at_height(center, client.camera.focus_elevation_meters);
    let after = client.camera.screen_to_world_at_height(
        ScreenPoint {
            x: center.x + delta[0],
            y: center.y + delta[1],
        },
        client.camera.focus_elevation_meters,
    );
    client.camera.center = [
        client.camera.center[0] + before[0] - after[0],
        client.camera.center[1] + before[1] - after[1],
    ];
    client.camera = client.camera.clamp_center(client.config);
}

pub(super) fn edge_pan(client: &mut Client, delta_ms: f64) {
    if client.drag.as_ref().is_some_and(|drag| drag.middle) {
        return;
    }
    let Some(pointer) = client.pointer else {
        return;
    };
    let band = 16.0 * dpr();
    let width = client.camera.viewport[0];
    let height = client.camera.viewport[1];
    let edge = [
        if pointer.x < band {
            -(1.0 - pointer.x / band).clamp(0.0, 1.0)
        } else if pointer.x > width - band {
            ((pointer.x - (width - band)) / band).clamp(0.0, 1.0)
        } else {
            0.0
        },
        if pointer.y < band {
            -(1.0 - pointer.y / band).clamp(0.0, 1.0)
        } else if pointer.y > height - band {
            ((pointer.y - (height - band)) / band).clamp(0.0, 1.0)
        } else {
            0.0
        },
    ];
    let length = (edge[0] * edge[0] + edge[1] * edge[1]).sqrt();
    if length == 0.0 {
        return;
    }
    let scale = 900.0 * dpr() * delta_ms / 1_000.0 / length;
    pan(client, [edge[0] * scale, edge[1] * scale]);
}

pub(super) fn install(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let canvas = shared.borrow().canvas.clone();
    let down = shared.clone();
    let ondown = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
        event.prevent_default();
        let mut client = down.borrow_mut();
        let target = point(&client, &event);
        if event.button() == 2 {
            send_order(&mut client, target);
            return;
        }
        let middle = event.button() == 1;
        if event.button() != 0 && !middle {
            return;
        }
        let _ = client.canvas.set_pointer_capture(event.pointer_id());
        client.drag = Some(Drag {
            start: target,
            current: target,
            middle,
            center: client.camera.center,
        });
    });
    canvas.add_event_listener_with_callback("pointerdown", ondown.as_ref().unchecked_ref())?;
    ondown.forget();

    let moving = shared.clone();
    let onmove = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
        let mut client = moving.borrow_mut();
        let target = point(&client, &event);
        client.pointer = Some(target);
        let (start, center, middle) = {
            let Some(drag) = client.drag.as_mut() else {
                return;
            };
            drag.current = target;
            (drag.start, drag.center, drag.middle)
        };
        if middle {
            let before = client
                .camera
                .screen_to_world_at_height(start, client.camera.focus_elevation_meters);
            let after = client
                .camera
                .screen_to_world_at_height(target, client.camera.focus_elevation_meters);
            client.camera.center = [
                center[0] + before[0] - after[0],
                center[1] + before[1] - after[1],
            ];
            client.camera = client.camera.clamp_center(client.config);
            subscribe(&mut client);
        }
    });
    canvas.add_event_listener_with_callback("pointermove", onmove.as_ref().unchecked_ref())?;
    onmove.forget();

    let released = shared.clone();
    let onup = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
        let mut client = released.borrow_mut();
        let target = point(&client, &event);
        if let Some(drag) = client.drag.take() {
            let moved = (drag.current.x - drag.start.x).abs() > 4.0
                || (drag.current.y - drag.start.y).abs() > 4.0;
            if !drag.middle {
                client.selected = if moved {
                    pick_box(&client, drag.start, target)
                } else {
                    pick(&client, target)
                };
            }
        }
        let _ = client.canvas.release_pointer_capture(event.pointer_id());
    });
    canvas.add_event_listener_with_callback("pointerup", onup.as_ref().unchecked_ref())?;
    canvas.add_event_listener_with_callback("pointercancel", onup.as_ref().unchecked_ref())?;
    onup.forget();
    let left = shared.clone();
    let onleave = Closure::<dyn FnMut(PointerEvent)>::new(move |_| {
        left.borrow_mut().pointer = None;
    });
    canvas.add_event_listener_with_callback("pointerleave", onleave.as_ref().unchecked_ref())?;
    onleave.forget();
    let context = Closure::<dyn FnMut(Event)>::new(move |event: Event| event.prevent_default());
    canvas.add_event_listener_with_callback("contextmenu", context.as_ref().unchecked_ref())?;
    context.forget();

    let zoomed = shared.clone();
    let onwheel = Closure::<dyn FnMut(WheelEvent)>::new(move |event: WheelEvent| {
        event.prevent_default();
        let mut client = zoomed.borrow_mut();
        let bounds = client.canvas.get_bounding_client_rect();
        let target = ScreenPoint {
            x: (f64::from(event.client_x()) - bounds.left()) * dpr(),
            y: (f64::from(event.client_y()) - bounds.top()) * dpr(),
        };
        let delta = match event.delta_mode() {
            1 => event.delta_y() * 16.0,
            2 => event.delta_y() * client.camera.viewport[1],
            _ => event.delta_y(),
        };
        let zoom = client.camera.zoom * (-delta * 0.0015).exp();
        client.camera = client
            .camera
            .zoom_around(target, zoom)
            .clamp_center(client.config);
        subscribe(&mut client);
    });
    canvas.add_event_listener_with_callback("wheel", onwheel.as_ref().unchecked_ref())?;
    onwheel.forget();

    let keyed = shared.clone();
    let onkey = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
        let mut client = keyed.borrow_mut();
        match event.key().as_str() {
            "Escape" => {
                client.selected = None;
                client.drag = None;
            }
            "g" | "G" => client.grid = !client.grid,
            "Home" => center_on_primary(&mut client),
            "ArrowLeft" | "a" => pan(&mut client, [-60.0, 0.0]),
            "ArrowRight" | "d" => pan(&mut client, [60.0, 0.0]),
            "ArrowUp" | "w" => pan(&mut client, [0.0, -60.0]),
            "ArrowDown" | "s" => pan(&mut client, [0.0, 60.0]),
            _ => return,
        }
        event.prevent_default();
        subscribe(&mut client);
    });
    shared
        .borrow()
        .document
        .add_event_listener_with_callback("keydown", onkey.as_ref().unchecked_ref())?;
    onkey.forget();

    for (id, action) in [
        ("grid", 0_u8),
        ("recenter", 1),
        ("reconnect", 2),
        ("fullscreen", 3),
    ] {
        if let Some(button) = shared.borrow().document.get_element_by_id(id) {
            let target = shared.clone();
            let callback = Closure::<dyn FnMut(Event)>::new(move |_| match action {
                0 => {
                    let grid = target.borrow().grid;
                    target.borrow_mut().grid = !grid;
                }
                1 => center_on_primary(&mut target.borrow_mut()),
                2 => reconnect(target.clone()),
                3 => {
                    let _ = target.borrow().canvas.request_fullscreen();
                }
                _ => {}
            });
            button.add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())?;
            callback.forget();
        }
    }
    Ok(())
}
