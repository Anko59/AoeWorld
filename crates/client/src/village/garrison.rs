//! Shelter controls send ordinary owned-unit orders; occupants arrive on the wire.
use super::{Client, construction, orders::send, owner};
use aoe_core::ScreenPoint;
use aoe_protocol::{
    BuildingKind, ConstructionState, GameplayBuildingState, GameplayClientMessage as Message,
};
use wasm_bindgen::prelude::wasm_bindgen;

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
        && super::selection::owned_units(client).any(|unit| b.kind.admits(unit.kind)))
    .then_some(b)
}

pub(in super::super) fn interact(client: &mut Client, point: ScreenPoint) -> bool {
    let Some(b) = target(client, point) else {
        return false;
    };
    let (building_id, kind) = (b.id, b.kind);
    let ids: Vec<_> = super::selection::owned_units(client)
        .filter(|unit| kind.admits(unit.kind))
        .map(|unit| unit.id)
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

pub(super) fn shelter_controls(building: Option<&GameplayBuildingState>) -> (bool, bool) {
    let Some(building) = building else {
        return (false, false);
    };
    let bell = building.kind == BuildingKind::TownCenter
        && building.construction == ConstructionState::Complete;
    let back_to_work = building
        .garrison
        .iter()
        .any(|unit| unit.kind == aoe_protocol::UnitKind::Villager);
    (bell, back_to_work)
}

pub(super) fn present(client: &Client) {
    let b = client.village.buildings.iter().find(|b| {
        client.selected.units.is_empty()
            && Some(b.id) == client.selected.building_id
            && b.player == owner(client)
    });
    let (bell, back_to_work) = shelter_controls(b);
    let mut values = vec![
        b.filter(|b| b.construction == ConstructionState::Complete)
            .map_or(0.0, |b| f64::from(b.kind.garrison_capacity())),
        f64::from(u8::from(super::selection::owned_units(client).any(
            |unit| {
                BuildingKind::Castle.admits(unit.kind) || BuildingKind::TownCenter.admits(unit.kind)
            },
        ))),
        f64::from(u8::from(bell)),
        f64::from(u8::from(back_to_work)),
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

pub(super) fn menu_action(client: &mut Client, code: u32, target: aoe_core::EntityId) {
    if code == 0 {
        construction::cancel(client);
        client.village.move_cursor = false;
        client.village.garrison_cursor = true;
        let _ = client.canvas.set_attribute("data-order-mode", "garrison");
        return;
    }
    let Some(building_id) = client.selected.building_id else {
        return;
    };
    let sequence = client.next_sequence;
    let message = match code {
        1 | 4 => Message::UngarrisonOrder {
            sequence,
            building_id,
            entity_id: (code == 4).then_some(target),
        },
        2 => Message::BackToWorkOrder {
            sequence,
            building_id,
        },
        3 => Message::TownBellOrder {
            sequence,
            building_id,
        },
        _ => return,
    };
    send(client, message);
}
