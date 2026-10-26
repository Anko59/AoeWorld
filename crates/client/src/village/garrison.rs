//! Shelter controls send ordinary owned-unit orders; occupants arrive on the wire.
use super::{Client, construction, orders::send, owner};
use aoe_core::ScreenPoint;
use aoe_protocol::{ConstructionState, GameplayBuildingState, GameplayClientMessage as Message};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};

#[wasm_bindgen(inline_js = "export function garrison_update(v) { window.aoeGarrison?.update(v); }")]
extern "C" {
    fn garrison_update(v: &[f64]);
}

pub(super) fn target(client: &Client, point: ScreenPoint) -> Option<&GameplayBuildingState> {
    if client.village.move_cursor {
        return None;
    }
    let b = construction::target(client, point)?;
    (b.player == owner(client)
        && b.construction == ConstructionState::Complete
        && (b.hp == b.max_hp || client.village.garrison_cursor)
        && client.selected.units.iter().any(|id| {
            client
                .units
                .get(id)
                .is_some_and(|u| u.player == owner(client) && u.hp > 0 && b.kind.admits(u.kind))
        }))
    .then_some(b)
}

pub(in super::super) fn interact(client: &mut Client, point: ScreenPoint) -> bool {
    let Some(b) = target(client, point) else {
        return false;
    };
    let (building_id, kind) = (b.id, b.kind);
    let ids: Vec<_> = client
        .selected
        .units
        .iter()
        .copied()
        .filter(|id| {
            client
                .units
                .get(id)
                .is_some_and(|u| u.player == owner(client) && u.hp > 0 && kind.admits(u.kind))
        })
        .collect();
    for entity_id in ids {
        let sequence = client.next_sequence;
        send(
            client,
            Message::GarrisonOrder {
                sequence,
                entity_id,
                building_id,
            },
        );
    }
    client.village.garrison_cursor = false;
    let _ = client.canvas.set_attribute("data-order-mode", "");
    true
}

pub(super) fn present(client: &Client) {
    let b = client
        .village
        .buildings
        .iter()
        .find(|b| Some(b.id) == client.selected.building_id && b.player == owner(client));
    let mut values = vec![
        b.map_or(0.0, |b| f64::from(b.kind.garrison_capacity())),
        f64::from(u8::from(client.selected.units.iter().any(|id| {
            client
                .units
                .get(id)
                .is_some_and(|u| u.hp > 0 && u.player == owner(client))
        }))),
    ];
    if let Some(b) = b {
        for u in &b.garrison {
            values.extend([
                f64::from(u.id.0),
                u.kind as u8 as f64,
                f64::from(u.hp),
                f64::from(u.max_hp),
            ]);
        }
    }
    garrison_update(&values);
}

pub(super) fn install(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    for id in ["garrison", "ungarrison", "garrison-units", "build-castle"] {
        let Some(button) = shared.borrow().document.get_element_by_id(id) else {
            continue;
        };
        let target = shared.clone();
        let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            let mut client = target.borrow_mut();
            if id == "build-castle" {
                construction::arm(&mut client, aoe_protocol::BuildingKind::Castle);
                return;
            }
            if id == "garrison" {
                construction::cancel(&mut client);
                client.village.move_cursor = false;
                client.village.garrison_cursor = true;
                let _ = client.canvas.set_attribute("data-order-mode", "garrison");
                return;
            }
            let Some(building_id) = client.selected.building_id else {
                return;
            };
            let entity_id = if id == "garrison-units" {
                event
                    .target()
                    .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                    .and_then(|element| element.closest("button[data-unit-id]").ok().flatten())
                    .and_then(|element| element.get_attribute("data-unit-id"))
                    .and_then(|id| id.parse::<u32>().ok())
                    .map(aoe_core::EntityId)
            } else {
                None
            };
            if id == "garrison-units" && entity_id.is_none() {
                return;
            }
            let sequence = client.next_sequence;
            send(
                &mut client,
                Message::UngarrisonOrder {
                    sequence,
                    building_id,
                    entity_id,
                },
            );
        });
        button.add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())?;
        callback.forget();
    }
    Ok(())
}
