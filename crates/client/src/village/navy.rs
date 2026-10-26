//! Manual river crossing: nearby land units load, ships sail, then unload on a bank.
use super::{Client, orders::send, owner};
use aoe_core::{EntityId, ScreenPoint, TileCoord};
use aoe_protocol::{GameplayClientMessage as Message, UnitKind};

fn selected_transport(client: &Client) -> Option<EntityId> {
    client.selected.units.iter().copied().find(|id| {
        client.units.get(id).is_some_and(|u| {
            u.kind == UnitKind::TransportShip && u.player == owner(client) && u.hp > 0
        })
    })
}

pub(super) fn arm_unload(client: &mut Client) {
    if selected_transport(client).is_some() {
        client.village.move_cursor = false;
        client.village.unload_cursor = true;
        let _ = client.canvas.set_attribute("data-order-mode", "unload");
        client.status = "Right-click an adjacent bank to unload".into();
    }
}

pub(super) fn at_pointer(client: &mut Client, point: ScreenPoint, forced_move: bool) -> bool {
    if forced_move {
        client.village.unload_cursor = false;
        return false;
    }
    if client.village.unload_cursor {
        client.village.unload_cursor = false;
        let _ = client.canvas.set_attribute("data-order-mode", "");
        let Some(transport_id) = selected_transport(client) else {
            return false;
        };
        if let Some(p) = crate::playground::game::map::world_at_screen(client, point) {
            let sequence = client.next_sequence;
            send(
                client,
                Message::UnloadOrder {
                    sequence,
                    transport_id,
                    destination: TileCoord::new(p[0].floor() as i32, p[1].floor() as i32),
                },
            );
        }
        return true;
    }
    let Some((transport_id, false, false)) = super::picking::actor(client, point) else {
        return false;
    };
    if !client
        .units
        .get(&transport_id)
        .is_some_and(|u| u.kind == UnitKind::TransportShip && u.player == owner(client) && u.hp > 0)
    {
        return false;
    }
    let capacity = client
        .units
        .get(&transport_id)
        .map_or(0, |u| 5_usize.saturating_sub(usize::from(u.cargo_count)));
    let unit_ids = client
        .selected
        .units
        .iter()
        .copied()
        .filter(|id| {
            client
                .units
                .get(id)
                .is_some_and(|u| !u.kind.is_naval() && u.player == owner(client) && u.hp > 0)
        })
        .take(capacity)
        .collect::<Vec<_>>();
    if unit_ids.is_empty() {
        return false;
    }
    let sequence = client.next_sequence;
    send(
        client,
        Message::LoadOrder {
            sequence,
            unit_ids,
            transport_id,
        },
    );
    true
}

pub(super) fn state(client: &Client) -> [f64; 2] {
    let cargo = selected_transport(client)
        .and_then(|id| client.units.get(&id))
        .map(|u| u.cargo_count);
    [
        f64::from(u8::from(cargo.is_some())),
        cargo.map_or(0.0, f64::from),
    ]
}
