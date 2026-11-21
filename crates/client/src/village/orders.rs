use super::{Client, owner};
use crate::playground::game::map;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, ScreenPoint, WorldPosition};
use aoe_protocol::{GameplayClientMessage as Message, UnitKind, encode_gameplay_client};

pub(super) fn send(client: &mut Client, message: Message) -> bool {
    client
        .village
        .build_admission
        .sent(crate::playground::game::now());
    client.last_error = None;
    let sequence = client.next_sequence;
    client.next_sequence = sequence.saturating_add(1);
    if let Ok(bytes) = encode_gameplay_client(&message)
        && crate::playground::game::network::send(client, &bytes)
    {
        client.order_feedback.record(sequence, None);
        true
    } else {
        false
    }
}

pub(super) fn cancel_training(client: &mut Client) {
    let Some(building) =
        super::production::selected(client).filter(|building| building.player == owner(client))
    else {
        return;
    };
    let building_id = building.id;
    let sequence = client.next_sequence;
    send(
        client,
        Message::CancelTrainingOrder {
            sequence,
            building_id,
        },
    );
}

pub(in super::super) fn cancel_move_mode(client: &mut Client) {
    client.village.clear_move_cursor();
    let _ = client.canvas.set_attribute("data-order-mode", "");
}

pub(in super::super) fn stop(client: &mut Client) {
    client.village.garrison_cursor = false;
    cancel_move_mode(client);
    super::waypoints::clear_order_pings();
    let owned = client
        .selected
        .units
        .iter()
        .copied()
        .filter(|id| {
            client
                .units
                .get(id)
                .is_some_and(|u| u.player == owner(client) && u.hp > 0)
                || client.village.animals.iter().any(|a| {
                    a.id == *id
                        && a.kind == aoe_protocol::AnimalKind::Sheep
                        && a.hp > 0
                        && a.owner == Some(owner(client))
                })
        })
        .collect::<Vec<_>>();
    for unit_ids in owned.chunks(aoe_core::MAX_FORMATION_MEMBERS) {
        send(
            client,
            Message::StopOrder {
                sequence: client.next_sequence,
                unit_ids: unit_ids.to_vec(),
            },
        );
    }
}

pub(in super::super) fn at_pointer(client: &mut Client, point: ScreenPoint, queued: bool) {
    let contextual = !queued && !client.village.attack_move && !client.village.patrol_move;
    if client.selected.units.is_empty()
        && ((contextual && building_attack_at_pointer(client, point))
            || (client.selected.building_id.is_some() && super::rally::at_pointer(client, point)))
    {
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
    let resource = super::picking::resource(client, point);
    if contextual
        && actor.is_none()
        && resource.is_none()
        && enemy.is_none()
        && (super::commerce::trade_interact(client, point)
            || super::garrison::interact(client, point)
            || super::construction::interact(client, point))
    {
        return;
    }
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
    if std::mem::take(&mut client.village.general_only) && !queued {
        let generals: Vec<_> = super::selection::owned_units(client)
            .filter(|unit| unit.kind == UnitKind::General)
            .map(|unit| unit.id)
            .collect();
        for entity_id in generals {
            let sequence = client.next_sequence;
            let message = Message::GeneralOrder {
                sequence,
                entity_id,
                command: aoe_protocol::GeneralCommand::MoveAlone { destination },
            };
            send(client, message);
        }
        cancel_move_mode(client);
        return;
    }
    let mut moves = client
        .village
        .animals
        .iter()
        .filter(|a| {
            !client.village.patrol_move
                && a.hp > 0
                && a.owner == Some(owner(client))
                && client.selected.units.contains(&a.id)
        })
        .map(|a| a.id)
        .collect::<Vec<_>>();
    let mut attacks = Vec::new();
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
    for id in client.selected.units.clone() {
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
        if client.village.patrol_move {
            if can_choose_stance(unit.kind) {
                send(
                    client,
                    Message::PatrolOrder {
                        sequence,
                        entity_id: id,
                        destination,
                    },
                );
            }
            continue;
        }
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
            (ClickOrder::Attack, Some(_), _) => {
                attacks.push(id);
                continue;
            }
            (ClickOrder::Gather, _, Some((resource_id, _))) => Message::GatherOrder {
                sequence,
                entity_id: id,
                resource_id,
            },
            _ => {
                moves.push(id);
                continue;
            }
        };
        send(client, message);
    }
    if let Some(target) = animal.or(enemy) {
        send_attack(client, attacks, target);
    }
    let move_sequence = client.next_sequence;
    if let Some(message) = super::selection_orders::movement(client, moves, destination, queued) {
        send(client, message);
        let blocker = map::feedback::blocker_at(client, destination.tile_floor());
        client.order_feedback.record(move_sequence, blocker);
    }
    if client.village.attack_move || client.village.patrol_move {
        super::waypoints::ping(client, destination);
    }
    if !queued {
        cancel_move_mode(client);
    }
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
                        | aoe_protocol::BuildingKind::GuardTower
                        | aoe_protocol::BuildingKind::Keep
                        | aoe_protocol::BuildingKind::TownCenter
                )
        })
        .map(|b| b.id)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return false;
    }
    send_attack(client, ids, target_id);
    true
}

fn send_attack(
    client: &mut Client,
    mut entity_ids: Vec<aoe_core::EntityId>,
    target_id: aoe_core::EntityId,
) {
    let sequence = client.next_sequence;
    let message = match entity_ids.len() {
        0 => return,
        1 => Message::AttackOrder {
            sequence,
            entity_id: entity_ids[0],
            target_id,
        },
        _ => {
            entity_ids.sort_unstable();
            Message::AttackSelectionOrder {
                sequence,
                entity_ids,
                target_id,
            }
        }
    };
    send(client, message);
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
        if select {
            client.selected.building(id);
        }
    }
}

pub(super) fn can_form(kind: UnitKind) -> bool {
    !kind.is_naval()
        && (can_choose_stance(kind) || matches!(kind, UnitKind::General | UnitKind::Monk))
}

pub(super) fn can_choose_stance(kind: UnitKind) -> bool {
    !matches!(kind, UnitKind::Villager | UnitKind::General) && can_attack_move(kind)
}

pub(super) fn can_attack_move(kind: UnitKind) -> bool {
    crate::building_ui::content::rules().units[kind as usize]
        .combat
        .attack
        > 0
}

fn military(client: &Client) -> Vec<aoe_core::EntityId> {
    let mut military = super::selection::owned_units(client)
        .filter(|unit| can_form(unit.kind))
        .map(|unit| unit.id)
        .take(aoe_core::MAX_FORMATION_MEMBERS)
        .collect::<Vec<_>>();
    military.sort_unstable();
    military
}

pub(super) fn formation(client: &mut Client, index: u32) {
    let Some(&form) = [
        aoe_core::Formation::Line,
        aoe_core::Formation::Box,
        aoe_core::Formation::Staggered,
        aoe_core::Formation::Flank,
    ]
    .get(index as usize) else {
        return;
    };
    client.village.formation = form;
    let unit_ids = military(client);
    if unit_ids.len() > 1 {
        send(
            client,
            Message::FormationOrder {
                sequence: client.next_sequence,
                unit_ids,
                formation: form,
                destination: None,
                queued: false,
                attack_move: false,
            },
        );
    }
}

/// Delete the selected units and buildings this player owns (never animals or
/// foreign entities). The server refuses the whole order if any id is foreign.
pub(in super::super) fn delete(client: &mut Client) {
    let owner = owner(client);
    let mut ids: Vec<_> = super::selection::owned_units(client)
        .map(|unit| unit.id)
        .collect();
    let mut buildings = client.selected.buildings.clone();
    buildings.extend(client.selected.building_id);
    ids.extend(buildings.into_iter().filter(|id| {
        client
            .village
            .buildings
            .iter()
            .any(|b| b.id == *id && b.player == owner)
    }));
    ids.sort_unstable();
    ids.dedup();
    for chunk in ids.chunks(aoe_core::MAX_FORMATION_MEMBERS) {
        send(
            client,
            Message::DeleteOrder {
                sequence: client.next_sequence,
                entity_ids: chunk.to_vec(),
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
    let unit_ids = super::selection::owned_units(client)
        .filter(|unit| can_choose_stance(unit.kind))
        .map(|unit| unit.id)
        .collect::<Vec<_>>();
    for ids in unit_ids.chunks(aoe_core::MAX_FORMATION_MEMBERS) {
        send(
            client,
            Message::StanceOrder {
                sequence: client.next_sequence,
                unit_ids: ids.to_vec(),
                stance,
            },
        );
    }
}
