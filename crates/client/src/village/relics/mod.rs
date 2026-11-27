use super::{Client, owner};
use aoe_core::EntityId;
use aoe_protocol::{BuildingKind, GameplayClientMessage, GroundRelic, RelicAction, UnitKind};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(
    inline_js = "export function relic_hud(info, marks) { window.aoeRelics?.update(info, marks); }"
)]
extern "C" {
    fn relic_hud(info: &[f64], marks: &[f64]);
}

fn view(client: &Client) -> Option<&aoe_protocol::RelicView> {
    client
        .village
        .relics
        .as_ref()
        .filter(|view| view.revision == client.revision)
}

pub(crate) fn sync(client: &mut Client, view: aoe_protocol::RelicView) {
    let changed_actors = client
        .village
        .relics
        .as_ref()
        .is_none_or(|before| before.ground != view.ground || before.carriers != view.carriers);
    let before = client.village.relics.replace(view);
    if changed_actors {
        if let Some(before) = before {
            let village = &client.village;
            client.selected.units.retain(|id| {
                before.ground.binary_search_by_key(id, |r| r.id).is_err()
                    || village.knows_ground_relic(*id)
            });
        }
        client.rendered_frame.invalidate();
    }
}

pub(crate) fn ground(client: &Client) -> &[GroundRelic] {
    view(client).map_or(&[], |view| view.ground.as_slice())
}

pub(crate) fn carries(client: &Client, id: EntityId) -> bool {
    view(client).is_some_and(|view| view.carriers.binary_search(&id).is_ok())
}

fn count(client: &Client, monastery: EntityId) -> u16 {
    view(client)
        .and_then(|view| {
            let slot = view
                .monasteries
                .binary_search_by_key(&monastery, |m| m.id)
                .ok()?;
            Some(view.monasteries[slot].count)
        })
        .unwrap_or(0)
}

pub(super) fn pick_up(client: &mut Client, relic: EntityId) -> bool {
    if ground(client)
        .binary_search_by_key(&relic, |r| r.id)
        .is_err()
    {
        return false;
    }
    let monk = super::selection::owned_units(client)
        .find(|u| u.kind == UnitKind::Monk && !carries(client, u.id))
        .map(|u| u.id);
    monk.is_some_and(|monk| {
        super::orders::send(
            client,
            GameplayClientMessage::RelicOrder {
                sequence: client.next_sequence,
                action: RelicAction::PickUp { monk, relic },
            },
        )
    })
}

pub(super) fn update(client: &Client) {
    let viewer = owner(client);
    let selected_unit = client
        .selected
        .units
        .first()
        .copied()
        .filter(|_| client.selected.units.len() == 1);
    let carried = selected_unit.is_some_and(|id| carries(client, id));
    let owns_carrier = carried
        && selected_unit
            .and_then(|id| client.units.get(&id))
            .is_some_and(|u| u.player == viewer);
    let selected_monastery = client.selected.building_id.and_then(|id| {
        client
            .village
            .buildings
            .iter()
            .find(|b| b.id == id && b.kind == BuildingKind::Monastery)
    });
    let stored = selected_monastery.map_or(-1.0, |m| f64::from(count(client, m.id)));
    let owns_store = stored > 0.0 && selected_monastery.is_some_and(|m| m.player == viewer);
    let victory = view(client).and_then(|view| view.victory);
    let info = [
        f64::from(u8::from(carried)),
        stored,
        victory.map_or(-1.0, |v| f64::from(v.leader.0)),
        victory.map_or(-1.0, |v| f64::from(v.remaining_seconds)),
        f64::from(viewer.0),
        f64::from(u8::from(owns_carrier)),
        f64::from(u8::from(owns_store)),
    ];
    let marks = ground(client)
        .iter()
        .flat_map(|r| r.position.as_tiles())
        .collect::<Vec<_>>();
    relic_hud(&info, &marks);
}

pub(super) fn menu_action(client: &mut Client, code: u32) {
    let action = match code {
        0 => super::selection::owned_units(client)
            .find(|u| carries(client, u.id))
            .map(|u| RelicAction::Drop { monk: u.id }),
        1 => client
            .selected
            .building_id
            .filter(|&id| count(client, id) > 0)
            .map(|monastery| RelicAction::Eject { monastery }),
        _ => None,
    };
    if let Some(action) = action {
        super::orders::send(
            client,
            GameplayClientMessage::RelicOrder {
                sequence: client.next_sequence,
                action,
            },
        );
    }
}
