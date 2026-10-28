use super::{Client, Drag, center_on_primary, map, position_at, reconnect, send_order, subscribe};
use aoe_core::{Camera, ScreenPoint, WorldConfig};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{Event, PointerEvent, WheelEvent};

#[path = "playground_controls/groups.rs"]
pub(super) mod groups;
#[path = "playground_controls/navigation.rs"]
pub(super) mod navigation;

pub(super) fn map_changed(hash: Option<[u8; 32]>) {
    navigation::map_changed(hash);
}

// Bound color, depth, browser canvas and temporary presentation storage together.
// Retina density must not silently square memory on large/fullscreen windows.
const MAX_BACKING_PIXELS: f64 = 4_194_304.0;
const MAX_BACKING_AXIS: f64 = 4_096.0;

pub(super) fn backing_size(width: f64, height: f64, ratio: f64) -> [u32; 2] {
    let width = width.max(1.0);
    let height = height.max(1.0);
    let scale = ratio
        .clamp(0.25, 2.0)
        .min((MAX_BACKING_PIXELS / (width * height)).sqrt())
        .min(MAX_BACKING_AXIS / width.max(height));
    [
        (width * scale).floor().max(1.0) as u32,
        (height * scale).floor().max(1.0) as u32,
    ]
}

fn pixel_scale(client: &Client) -> f64 {
    f64::from(client.canvas.width()) / f64::from(client.canvas.client_width().max(1))
}

fn point(client: &mut Client, event: &PointerEvent) -> ScreenPoint {
    // Layout can change between animation callbacks. Synchronize projection
    // and backing pixels before retaining a pointer for the next frame.
    super::resize(client);
    let bounds = client.canvas.get_bounding_client_rect();
    ScreenPoint {
        x: (f64::from(event.client_x()) - bounds.left()) * f64::from(client.canvas.width())
            / bounds.width().max(1.0),
        y: (f64::from(event.client_y()) - bounds.top()) * f64::from(client.canvas.height())
            / bounds.height().max(1.0),
    }
}

fn pick(client: &Client, target: ScreenPoint) -> Option<aoe_core::EntityId> {
    super::village::picking::unit(client, target)
}

fn pick_box(client: &Client, start: ScreenPoint, end: ScreenPoint) -> Vec<aoe_core::EntityId> {
    super::village::selection::box_members(
        client.units.values().map(|u| {
            (
                u.id,
                u.player,
                map::screen_position(client, position_at(client, u.id)),
            )
        }),
        super::village::owner(client),
        start,
        end,
    )
}

fn pan_camera(mut camera: Camera, config: WorldConfig, delta: [f64; 2]) -> Camera {
    let center = ScreenPoint {
        x: camera.viewport[0] / 2.0,
        y: camera.viewport[1] / 2.0,
    };
    let before = camera.screen_to_world_at_height(center, camera.focus_elevation_meters);
    let after = camera.screen_to_world_at_height(
        ScreenPoint {
            x: center.x + delta[0],
            y: center.y + delta[1],
        },
        camera.focus_elevation_meters,
    );
    camera.center = [
        camera.center[0] + after[0] - before[0],
        camera.center[1] + after[1] - before[1],
    ];
    camera.clamp_center(config)
}

// Wheel zoom is browser UI arithmetic, not deterministic simulation state.
// Use the host exponential once per event instead of retaining a WASM libm root.
fn toggle_grid(client: &mut Client) {
    client.grid = !client.grid;
    if let Some(button) = client.document.get_element_by_id("grid") {
        let _ = button.set_attribute("aria-pressed", if client.grid { "true" } else { "false" });
    }
}

fn wheel_zoom(zoom: f64, delta: f64) -> f64 {
    zoom * js_sys::Math::exp(-delta * 0.0015)
}

fn pan(client: &mut Client, delta: [f64; 2]) {
    client.camera = pan_camera(client.camera, client.config, delta);
}

pub(super) fn edge_pan(client: &mut Client, delta_ms: f64) {
    navigation::frame(client, delta_ms);
    if client.drag.as_ref().is_some_and(|drag| drag.middle) {
        return;
    }
    let Some(pointer) = client.pointer else {
        return;
    };
    let band = 16.0 * pixel_scale(client);
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
    let scale = 1_600.0 * pixel_scale(client) * delta_ms / 1_000.0 / length;
    pan(client, [edge[0] * scale, edge[1] * scale]);
}

pub(super) fn install(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let canvas = shared.borrow().canvas.clone();
    let restored = shared.clone();
    let onrestored = Closure::<dyn FnMut(Event)>::new(move |_| {
        restored.borrow_mut().rendered_frame.clear();
    });
    canvas.add_event_listener_with_callback(
        "aoe-renderer-restored",
        onrestored.as_ref().unchecked_ref(),
    )?;
    onrestored.forget();
    let down = shared.clone();
    let ondown = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
        event.prevent_default();
        let mut client = down.borrow_mut();
        let _ = client.canvas.focus();
        let target = point(&mut client, &event);
        if event.button() == 2 && client.village.placement.is_some() {
            super::village::construction::cancel(&mut client);
            return;
        }
        if event.button() == 2 {
            // Ctrl queues: Firefox opens its context menu on Shift+right-click.
            super::village::orders::at_pointer(&mut client, target, event.ctrl_key());
            return;
        }
        if event.button() == 0 && client.village.placement.is_some() {
            let _ = client.canvas.set_pointer_capture(event.pointer_id());
            super::village::construction::place(&mut client, target, false, event.shift_key());
            return;
        }
        if event.button() == 0 && client.village.move_cursor {
            send_order(&mut client, target);
            return;
        }
        if event.button() == 0 && super::strategic::focus(&mut client, target) {
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
        let target = point(&mut client, &event);
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
    shared
        .borrow()
        .document
        .add_event_listener_with_callback("pointermove", onmove.as_ref().unchecked_ref())?;
    onmove.forget();

    let released = shared.clone();
    let onup = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
        let mut client = released.borrow_mut();
        let target = point(&mut client, &event);
        if event.type_() == "pointercancel" {
            client.drag = None;
            super::village::construction::cancel(&mut client);
            return;
        }
        if event.button() != 0 {
            client.drag = None;
            let _ = client.canvas.release_pointer_capture(event.pointer_id());
            return;
        }
        if client.village.garrison_cursor {
            client.drag = None;
            super::village::garrison::interact(&mut client, target);
            let _ = client.canvas.release_pointer_capture(event.pointer_id());
            return;
        }
        if client.village.placement.is_some() {
            client.drag = None;
            super::village::construction::place(&mut client, target, true, event.shift_key());
            let _ = client.canvas.release_pointer_capture(event.pointer_id());
            return;
        }
        if let Some(drag) = client.drag.take() {
            let moved = (drag.current.x - drag.start.x).abs() > 4.0
                || (drag.current.y - drag.start.y).abs() > 4.0;
            if !drag.middle {
                if moved {
                    let units = pick_box(&client, drag.start, target);
                    client.selected.units(units);
                } else {
                    let unit = pick(&client, target);
                    super::village::picking::click(&mut client, target, unit);
                }
            }
        }
        let _ = client.canvas.release_pointer_capture(event.pointer_id());
    });
    canvas.add_event_listener_with_callback("pointerup", onup.as_ref().unchecked_ref())?;
    canvas.add_event_listener_with_callback("pointercancel", onup.as_ref().unchecked_ref())?;
    onup.forget();
    let doubled = shared.clone();
    let ondouble = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
        let mut client = doubled.borrow_mut();
        let target = point(&mut client, &event);
        if let Some(unit) = pick(&client, target) {
            super::village::picking::double_click(&mut client, unit);
        } else {
            super::village::picking::double_building(&mut client);
        }
    });
    canvas.add_event_listener_with_callback("dblclick", ondouble.as_ref().unchecked_ref())?;
    ondouble.forget();
    let left = shared.clone();
    let onleave = Closure::<dyn FnMut(PointerEvent)>::new(move |_| {
        left.borrow_mut().pointer = None;
    });
    shared
        .borrow()
        .document
        .add_event_listener_with_callback("pointerleave", onleave.as_ref().unchecked_ref())?;
    onleave.forget();
    let context = Closure::<dyn FnMut(Event)>::new(move |event: Event| event.prevent_default());
    canvas.add_event_listener_with_callback("contextmenu", context.as_ref().unchecked_ref())?;
    context.forget();

    let zoomed = shared.clone();
    let onwheel = Closure::<dyn FnMut(WheelEvent)>::new(move |event: WheelEvent| {
        event.prevent_default();
        let mut client = zoomed.borrow_mut();
        super::resize(&mut client);
        let bounds = client.canvas.get_bounding_client_rect();
        let target = ScreenPoint {
            x: (f64::from(event.client_x()) - bounds.left()) * f64::from(client.canvas.width())
                / bounds.width().max(1.0),
            y: (f64::from(event.client_y()) - bounds.top()) * f64::from(client.canvas.height())
                / bounds.height().max(1.0),
        };
        let delta = match event.delta_mode() {
            1 => event.delta_y() * 16.0,
            2 => event.delta_y() * client.camera.viewport[1],
            _ => event.delta_y(),
        };
        let zoom = wheel_zoom(client.camera.zoom, delta);
        client.camera = client
            .camera
            .zoom_around(target, zoom)
            .clamp_center(client.config);
        subscribe(&mut client);
    });
    canvas.add_event_listener_with_callback("wheel", onwheel.as_ref().unchecked_ref())?;
    onwheel.forget();

    let keyed = shared.clone();
    let onkey = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        let action = navigation::gameplay_key(&event);
        if action == 0 {
            return;
        }
        let mut client = keyed.borrow_mut();
        match action {
            1 => {
                if client.village.placement.is_some() || client.village.garrison_cursor {
                    super::village::construction::cancel(&mut client);
                } else {
                    client.selected = Default::default();
                }
                client.village.move_cursor = false;
                client.village.unload_cursor = false;
                client.village.attack_move = false;
                client.drag = None;
            }
            6 if client.village.repeat_placement => {
                super::village::construction::cancel(&mut client)
            }
            2 => toggle_grid(&mut client),
            3 => center_on_primary(&mut client),
            4 => super::village::orders::train(&mut client, false),
            5 => super::village::orders::focus_town_center(&mut client, true),
            7 | 8 => super::village::selection::idle(&mut client, action == 8),
            10..=39 => groups::key(&mut client, action),
            _ => return,
        }
        subscribe(&mut client);
    });
    shared
        .borrow()
        .document
        .add_event_listener_with_callback("keydown", onkey.as_ref().unchecked_ref())?;
    shared
        .borrow()
        .document
        .add_event_listener_with_callback("keyup", onkey.as_ref().unchecked_ref())?;
    if let Some(window) = web_sys::window() {
        window.add_event_listener_with_callback("blur", onkey.as_ref().unchecked_ref())?;
    }
    onkey.forget();

    for (id, action) in [
        ("grid", 0_u8),
        ("recenter", 1),
        ("reconnect", 2),
        ("fullscreen", 3),
        ("town-center", 4),
    ] {
        if let Some(button) = shared.borrow().document.get_element_by_id(id) {
            let target = shared.clone();
            let callback = Closure::<dyn FnMut(Event)>::new(move |_| match action {
                0 => toggle_grid(&mut target.borrow_mut()),
                1 => center_on_primary(&mut target.borrow_mut()),
                2 => reconnect(target.clone()),
                3 => {
                    if let Some(game) = target.borrow().document.get_element_by_id("playground") {
                        let _ = game.request_fullscreen();
                    }
                }
                4 => super::village::orders::focus_town_center(&mut target.borrow_mut(), true),
                _ => {}
            });
            button.add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())?;
            callback.forget();
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "playground_controls/tests/camera_tests.rs"]
mod tests;
