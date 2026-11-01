use super::{Client, owner};
use crate::playground::game::map;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, ScreenPoint, WorldPosition};
use aoe_protocol::{GameplayClientMessage as Message, UnitKind, encode_gameplay_client};

pub(super) fn send(client: &mut Client, message: Message) {
    client.last_error = None;
    let sequence = client.next_sequence;
    client.next_sequence = sequence.saturating_add(1);
    if let Ok(bytes) = encode_gameplay_client(&message)
        && crate::playground::game::network::send(client, &bytes)
    {
        client.order_feedback.record(sequence, None);
    }
}

pub(in super::super) fn train(client: &mut Client, cancel: bool) {
    let Some(id) = super::production::selected(client).map(|building| building.id) else {
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
    if !cancel {
        let kind = client
            .village
            .buildings
            .iter()
            .find(|b| b.id == id)
            .map(|b| {
                crate::building_ui::availability::primary_training(
                    b,
                    client
                        .village
                        .players
                        .iter()
                        .find(|p| p.player == owner(client)),
                )
            })
            .unwrap_or(UnitKind::Villager);
        super::production::train(client, kind);
        return;
    }
    let sequence = client.next_sequence;
    send(
        client,
        Message::CancelTrainingOrder {
            sequence,
            building_id: id,
        },
    );
}

pub(super) fn train_kind(client: &mut Client, kind: UnitKind) {
    super::production::train(client, kind);
}

pub(in super::super) fn stop(client: &mut Client) {
    client.village.garrison_cursor = false;
    client.village.move_cursor = false;
    client.village.attack_move = false;
    let _ = client.canvas.set_attribute("data-order-mode", "");
    for id in client.selected.units.clone() {
        let owned = client
            .units
            .get(&id)
            .is_some_and(|u| u.player == owner(client) && u.hp > 0)
            || client.village.animals.iter().any(|a| {
                a.id == id
                    && a.kind == aoe_protocol::AnimalKind::Sheep
                    && a.hp > 0
                    && a.owner == Some(owner(client))
            });
        if owned {
            let sequence = client.next_sequence;
            send(
                client,
                Message::StopOrder {
                    sequence,
                    entity_id: id,
                },
            );
        }
    }
}

pub(in super::super) fn at_pointer(client: &mut Client, point: ScreenPoint, queued: bool) {
    if !queued && building_attack_at_pointer(client, point) {
        return;
    }
    if client.selected.building_id.is_some() && super::rally::at_pointer(client, point) {
        return;
    }
    let forced_move = queued || client.village.move_cursor;
    if super::navy::at_pointer(client, point, forced_move) {
        return;
    }
    let actor = super::picking::actor(client, point);
    let enemy = actor
        .filter(|a| a.2)
        .map(|a| a.0)
        .or_else(|| super::picking::enemy_building(client, point));
    let animal = actor.filter(|a| a.1).map(|a| a.0);
    if !queued
        && enemy.is_none()
        && animal.is_none()
        && (super::garrison::interact(client, point)
            || super::construction::interact(client, point))
    {
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
    for id in client.selected.units.clone() {
        if client
            .village
            .animals
            .iter()
            .any(|a| a.id == id && a.hp > 0 && a.owner == Some(owner(client)))
        {
            let sequence = client.next_sequence;
            send(
                client,
                Message::MoveOrder {
                    sequence,
                    entity_id: id,
                    destination,
                },
            );
        }
    }
    let group_gathers = resource.is_some()
        && !queued
        && animal.is_none()
        && enemy.is_none()
        && !client.village.move_cursor
        && client.selected.units.iter().any(|id| {
            client.units.get(id).is_some_and(|u| {
                matches!(u.kind, UnitKind::Villager | UnitKind::FishingShip)
                    && u.player == owner(client)
            })
        });
    let mut military = military(client);
    if group_gathers {
        military.retain(|id| client.units[id].kind != UnitKind::Scout);
    }
    let formation_move = military.len() > 1
        && (queued || client.village.move_cursor || (enemy.is_none() && animal.is_none()));
    if formation_move {
        let sequence = client.next_sequence;
        let message = Message::FormationOrder {
            sequence,
            unit_ids: military.clone(),
            formation: client.village.formation,
            destination: Some(destination),
            queued,
            attack_move: client.village.attack_move,
        };
        send(client, message);
    }
    for id in client.selected.units.clone() {
        if formation_move && military.binary_search(&id).is_ok() {
            continue;
        }
        let Some(unit) = client
            .units
            .get(&id)
            .filter(|u| u.player == owner(client) && u.hp > 0)
        else {
            continue;
        };
        if group_gathers && unit.kind == UnitKind::Scout {
            continue;
        }
        let sequence = client.next_sequence;
        use crate::unit_visuals::{ClickOrder, click_order};
        let choice = click_order(
            unit.kind,
            client.village.move_cursor || queued,
            animal.is_some(),
            enemy.is_some(),
            resource.is_some(),
        );
        let message = match (choice, animal.or(enemy), resource) {
            (ClickOrder::Hunt, Some(target), _) => Message::HuntOrder {
                sequence,
                unit_ids: vec![id],
                animal_id: target,
            },
            (ClickOrder::Attack, Some(target), _) => Message::AttackOrder {
                sequence,
                entity_id: id,
                target_id: target,
            },
            (ClickOrder::Gather, _, Some((resource_id, _))) => Message::GatherOrder {
                sequence,
                entity_id: id,
                resource_id,
            },
            _ if queued => Message::QueueMoveOrder {
                sequence,
                entity_id: id,
                destination,
            },
            _ => Message::MoveOrder {
                sequence,
                entity_id: id,
                destination,
            },
        };
        send(client, message);
        let blocker = map::feedback::blocker_at(client, destination.tile_floor());
        client.order_feedback.record(sequence, blocker);
    }
    if client.village.attack_move {
        super::waypoints::ping(client, destination);
    }
    client.village.move_cursor = false;
    client.village.attack_move = false;
    let _ = client.canvas.set_attribute("data-order-mode", "");
}

/// Enemy clicks command defensive weapons; other clicks can still set rallies.
fn building_attack_at_pointer(client: &mut Client, point: ScreenPoint) -> bool {
    if client.selected.building_id.is_none() {
        return false;
    }
    let Some(target_id) = super::picking::actor(client, point)
        .filter(|a| a.2)
        .map(|a| a.0)
        .or_else(|| super::picking::enemy_building(client, point))
    else {
        return false;
    };
    let ids = client
        .village
        .buildings
        .iter()
        .filter(|b| {
            client.selected.has_building(b.id)
                && b.player == owner(client)
                && matches!(
                    b.kind,
                    aoe_protocol::BuildingKind::Castle
                        | aoe_protocol::BuildingKind::WatchTower
                        | aoe_protocol::BuildingKind::TownCenter
                )
        })
        .map(|b| b.id)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return false;
    }
    for entity_id in ids {
        let sequence = client.next_sequence;
        send(
            client,
            Message::AttackOrder {
                sequence,
                entity_id,
                target_id,
            },
        );
    }
    true
}

pub(in super::super) fn focus_town_center(client: &mut Client, select: bool) {
    if let Some((id, position)) = client
        .village
        .buildings
        .iter()
        .find(|b| b.player == owner(client) && b.kind == aoe_protocol::BuildingKind::TownCenter)
        .map(|b| (b.id, super::building_center(b)))
    {
        map::center_on_world(client, position);
        client.camera = client.camera.clamp_center(client.config);
        if select {
            client.selected.building(id);
        }
    }
}

fn military(client: &Client) -> Vec<aoe_core::EntityId> {
    let mut military = client
        .selected
        .units
        .iter()
        .copied()
        .filter(|id| {
            client.units.get(id).is_some_and(|u| {
                u.hp > 0
                    && u.player == owner(client)
                    && !matches!(u.kind, UnitKind::Villager | UnitKind::General)
                    && !u.kind.is_naval()
            })
        })
        .take(aoe_core::MAX_FORMATION_MEMBERS)
        .collect::<Vec<_>>();
    military.sort_unstable();
    military
}

pub(super) fn formation(client: &mut Client, index: u32) {
    let form = [
        aoe_core::Formation::Line,
        aoe_core::Formation::Box,
        aoe_core::Formation::Staggered,
        aoe_core::Formation::Flank,
    ][index as usize];
    client.village.formation = form;
    let unit_ids = military(client);
    if unit_ids.len() > 1 {
        let sequence = client.next_sequence;
        send(
            client,
            Message::FormationOrder {
                sequence,
                unit_ids,
                formation: form,
                destination: None,
                queued: false,
                attack_move: false,
            },
        );
    }
}

pub(super) fn stance(client: &mut Client, index: u32) {
    let Some(&stance) = [
        aoe_protocol::CombatStance::Aggressive,
        aoe_protocol::CombatStance::Defensive,
        aoe_protocol::CombatStance::StandGround,
        aoe_protocol::CombatStance::NoAttack,
    ]
    .get(index as usize) else {
        return;
    };
    let unit_ids = client
        .selected
        .units
        .iter()
        .copied()
        .filter(|id| {
            client.units.get(id).is_some_and(|u| {
                u.player == owner(client)
                    && !matches!(
                        u.kind,
                        UnitKind::Villager
                            | UnitKind::FishingShip
                            | UnitKind::TransportShip
                            | UnitKind::Monk
                            | UnitKind::Vassal
                            | UnitKind::General
                    )
            })
        })
        .collect::<Vec<_>>();
    for ids in unit_ids.chunks(aoe_core::MAX_FORMATION_MEMBERS) {
        let sequence = client.next_sequence;
        send(
            client,
            Message::StanceOrder {
                sequence,
                unit_ids: ids.to_vec(),
                stance,
            },
        );
    }
}
