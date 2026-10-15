use super::{Client, owner};
use crate::playground::game::map;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, ScreenPoint, WorldPosition};
use aoe_protocol::{
    GameplayClientMessage as Message, GameplayRole, UnitKind, encode_gameplay_client,
};

pub(super) fn send(client: &mut Client, message: Message) {
    if client.role != Some(GameplayRole::Controller) {
        return;
    }
    let sequence = client.next_sequence;
    client.next_sequence = sequence.saturating_add(1);
    if let Ok(bytes) = encode_gameplay_client(&message)
        && client
            .socket
            .as_ref()
            .is_some_and(|socket| socket.send_with_u8_array(&bytes).is_ok())
    {
        client.order_feedback.record(sequence, None);
    }
}

pub(in super::super) fn train(client: &mut Client, cancel: bool) {
    let Some(id) = client.selected.building_id else {
        return;
    };
    if !client
        .village
        .buildings
        .iter()
        .any(|b| b.id == id && b.player == owner(client))
    {
        return;
    }
    let sequence = client.next_sequence;
    send(
        client,
        if cancel {
            Message::CancelTrainingOrder {
                sequence,
                building_id: id,
            }
        } else {
            Message::TrainOrder {
                sequence,
                building_id: id,
                kind: UnitKind::Villager,
            }
        },
    );
}

pub(in super::super) fn stop(client: &mut Client) {
    for id in client.selected.units.clone() {
        if let Some(unit) = client.units.get(&id).filter(|u| u.player == owner(client)) {
            let destination = unit.position;
            let sequence = client.next_sequence;
            send(
                client,
                Message::MoveOrder {
                    sequence,
                    entity_id: id,
                    destination,
                },
            );
            client.village.set_job(id, None);
        }
    }
}

pub(in super::super) fn at_pointer(client: &mut Client, point: ScreenPoint) {
    if client.role != Some(GameplayRole::Controller) {
        return;
    }
    let resource = super::picking::resource(client, point);
    let Some(world) = map::world_at_screen(client, point) else {
        client.status = "terrain unavailable at pointer".into();
        return;
    };
    if world[0] < 0.0
        || world[1] < 0.0
        || world[0] >= f64::from(client.config.width_tiles)
        || world[1] >= f64::from(client.config.height_tiles)
    {
        return;
    }
    let Ok(destination) = WorldPosition::from_i64(
        (world[0] * f64::from(FIXED_SUBUNITS_PER_TILE)).round() as i64,
        (world[1] * f64::from(FIXED_SUBUNITS_PER_TILE)).round() as i64,
    ) else {
        return;
    };
    let destination = client.config.snap_ground_position(destination);
    let group_gathers = resource.is_some()
        && !client.village.move_cursor
        && client.selected.units.iter().any(|id| {
            client
                .units
                .get(id)
                .is_some_and(|u| u.kind == UnitKind::Villager && u.player == owner(client))
        });
    for id in client.selected.units.clone() {
        let Some(unit) = client.units.get(&id).filter(|u| u.player == owner(client)) else {
            continue;
        };
        if group_gathers && unit.kind == UnitKind::Scout {
            continue;
        }
        let sequence = client.next_sequence;
        let message = if unit.kind == UnitKind::Villager
            && !client.village.move_cursor
            && let Some((resource_id, kind)) = resource
        {
            client.village.set_job(id, Some(kind));
            Message::GatherOrder {
                sequence,
                entity_id: id,
                resource_id,
            }
        } else {
            client.village.set_job(id, None);
            Message::MoveOrder {
                sequence,
                entity_id: id,
                destination,
            }
        };
        send(client, message);
        let blocker = map::feedback::blocker_at(client, destination.tile_floor());
        client.order_feedback.record(sequence, blocker);
    }
    client.village.move_cursor = false;
    let _ = client.canvas.set_attribute("data-order-mode", "");
}

pub(in super::super) fn focus_town_center(client: &mut Client, select: bool) {
    if let Some((id, position)) = client
        .village
        .buildings
        .iter()
        .find(|b| b.player == owner(client))
        .map(|b| (b.id, super::building_center(b)))
    {
        map::center_on_world(client, position);
        client.camera = client.camera.clamp_center(client.config);
        if select {
            client.selected.building(id);
        }
    }
}
