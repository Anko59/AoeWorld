//! Contextual boarding, shore unloading and visible transport capacity.
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
        client.status = "Right-click a shore to sail there and unload".into();
    }
}

pub(super) fn at_pointer(client: &mut Client, point: ScreenPoint, forced_move: bool) -> bool {
    if forced_move {
        client.village.unload_cursor = false;
        return false;
    }
    let shore = selected_transport(client).is_some_and(|id| client.units[&id].cargo_count > 0)
        && crate::playground::game::map::world_at_screen(client, point).is_some_and(|p| {
            crate::playground::game::map::terrain_tile(
                client,
                p[0].floor() as i32,
                p[1].floor() as i32,
            )
            .is_some_and(|t| t.passable && t.water == aoe_map::WaterKind::None)
        });
    if client.village.unload_cursor || shore {
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
    let Some(transport_id) = boarding_target(client, point) else {
        return false;
    };
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

pub(super) fn state(client: &Client) -> Vec<f64> {
    let cargo = selected_transport(client)
        .and_then(|id| client.units.get(&id))
        .map(|u| u.cargo_count);
    let mut values = vec![
        f64::from(u8::from(cargo.is_some())),
        cargo.map_or(0.0, f64::from),
    ];
    let scale = f64::from(client.canvas.client_width()) / f64::from(client.canvas.width());
    for u in client.units.values().filter(|u| {
        u.kind == UnitKind::TransportShip
            && u.hp > 0
            && u.cargo_count > 0
            && (u.player == owner(client)
                || client
                    .fog
                    .state(u.position.as_tiles().map(|p| p.floor() as i32))
                    == 2)
    }) {
        let p = crate::playground::game::map::screen_position(client, u.position.as_tiles());
        values.extend([
            f64::from(u.id.0),
            p.x * scale,
            p.y * scale,
            f64::from(u.cargo_count),
        ]);
    }
    values
}

pub(super) fn boarding_cursor(
    kind: UnitKind,
    allied: bool,
    alive: bool,
    cargo: u8,
    passengers: bool,
) -> Option<f64> {
    (kind == UnitKind::TransportShip && allied && alive && passengers).then_some(if cargo < 5 {
        12.0
    } else {
        3.0
    })
}

pub(super) fn hover(client: &Client, point: ScreenPoint) -> Option<f64> {
    let (id, false, false) = super::picking::actor(client, point)? else {
        return None;
    };
    let u = client.units.get(&id)?;
    let passengers = client.selected.units.iter().any(|id| {
        client
            .units
            .get(id)
            .is_some_and(|p| p.player == owner(client) && p.hp > 0 && !p.kind.is_naval())
    });
    boarding_cursor(
        u.kind,
        u.player == owner(client),
        u.hp > 0,
        u.cargo_count,
        passengers,
    )
}

fn boarding_target(client: &Client, point: ScreenPoint) -> Option<EntityId> {
    (hover(client, point) == Some(12.0))
        .then(|| super::picking::actor(client, point).map(|a| a.0))
        .flatten()
}
