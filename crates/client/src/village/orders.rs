use super::{Client, owner};
use crate::playground::game::map;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, ScreenPoint, WorldPosition};
use aoe_protocol::{GameplayClientMessage as Message, UnitKind, encode_gameplay_client};

pub(super) fn send(client: &mut Client, message: Message) -> bool {
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

fn shows(client: &Client, mode: &str) -> bool {
    client.canvas.get_attribute("data-order-mode").as_deref() == Some(mode)
}

pub(in super::super) fn stop(client: &mut Client) {
    client.village.garrison_cursor = false;
    cancel_move_mode(client);
    super::waypoints::clear_order_pings();
    let owned = super::selection::commanded(client);
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
        // A building took the press: it must not leave a General to walk alone.
        client.village.general_only = false;
        if shows(client, "general-move") {
            cancel_move_mode(client);
        }
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
    if let Some(destination) = destination(client, world) {
        let resource = resource.map(|(id, _)| id);
        give(client, destination, queued, animal, enemy, resource);
    }
}

/// The orders a right-click on empty ground gives, at a point that may be far
/// off the screen: nothing is picked there. True when a unit order was sent.
pub(in super::super) fn at_world(client: &mut Client, world: [f64; 2], queued: bool) -> bool {
    let Some(destination) = destination(client, world) else {
        return false;
    };
    if client.selected.units.is_empty()
        && client.selected.building_id.is_some()
        && super::rally::at_ground(client, destination)
    {
        super::construction::cancel(client);
        return false;
    }
    client.village.unload_cursor = false;
    if shows(client, "unload") {
        let _ = client.canvas.set_attribute("data-order-mode", "");
    }
    if !queued {
        client.village.garrison_cursor = false;
    }
    give(client, destination, queued, None, None, None)
}

/// The ground a click orders to; none outside the world or for NaN.
fn destination(client: &Client, world: [f64; 2]) -> Option<WorldPosition> {
    let size = [client.config.width_tiles, client.config.height_tiles];
    let inside = |(at, tiles): (f64, i32)| (0.0..f64::from(tiles)).contains(&at);
    if !world.into_iter().zip(size).all(inside) {
        return None;
    }
    ground(client, world)
}

pub(super) fn ground(client: &Client, world: [f64; 2]) -> Option<WorldPosition> {
    let [x, y] = world.map(|v| (v * f64::from(FIXED_SUBUNITS_PER_TILE)).round() as i64);
    let position = WorldPosition::from_i64(x, y).ok()?;
    Some(client.config.snap_ground_position(position))
}

/// Orders the selection, also against what the click picked; true when a unit
/// was sent to the destination.
fn give(
    client: &mut Client,
    destination: WorldPosition,
    queued: bool,
    animal: Option<aoe_core::EntityId>,
    enemy: Option<aoe_core::EntityId>,
    resource: Option<u64>,
) -> bool {
    if std::mem::take(&mut client.village.general_only) && !queued {
        let generals: Vec<_> = super::selection::owned_units(client)
            .filter(|unit| unit.kind == UnitKind::General)
            .map(|unit| unit.id)
            .collect();
        let mut sent = false;
        for entity_ids in generals.chunks(aoe_core::MAX_FORMATION_MEMBERS) {
            let sequence = client.next_sequence;
            let message = if let [entity_id] = entity_ids {
                Message::GeneralOrder {
                    sequence,
                    entity_id: *entity_id,
                    command: aoe_protocol::GeneralCommand::MoveAlone { destination },
                }
            } else {
                Message::GeneralMoveAloneSelectionOrder {
                    sequence,
                    entity_ids: entity_ids.to_vec(),
                    destination,
                }
            };
            sent |= send(client, message);
        }
        cancel_move_mode(client);
        return sent;
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
    let mut patrols = Vec::new();
    let mut gathers = Vec::new();
    let mut hunts = Vec::new();
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
        if client.village.patrol_move {
            if can_choose_stance(unit.kind) {
                patrols.push(id);
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
        match (choice, animal.or(enemy), resource) {
            (ClickOrder::Hunt, Some(_), _) => hunts.push(id),
            (ClickOrder::Attack, Some(_), _) => attacks.push(id),
            (ClickOrder::Gather, _, Some(_)) => gathers.push(id),
            _ => moves.push(id),
        }
    }
    let mut sent = send_lists(client, patrols, |sequence, entity_ids| {
        Message::PatrolSelectionOrder {
            sequence,
            entity_ids,
            destination,
        }
    });
    if let Some(resource_id) = resource {
        send_lists(client, gathers, |sequence, entity_ids| {
            Message::GatherSelectionOrder {
                sequence,
                entity_ids,
                resource_id,
            }
        });
    }
    if let Some(animal_id) = animal.or(enemy) {
        send_lists(client, hunts, |sequence, unit_ids| Message::HuntOrder {
            sequence,
            unit_ids,
            animal_id,
        });
        send_attack(client, attacks, animal_id);
    }
    moves.sort_unstable();
    moves.dedup();
    for units in moves.chunks(aoe_core::MAX_FORMATION_MEMBERS) {
        let sequence = client.next_sequence;
        if let Some(message) =
            super::selection_orders::movement(client, units.to_vec(), destination, queued)
            && send(client, message)
        {
            sent = true;
            let blocker = map::feedback::blocker_at(client, destination.tile_floor());
            client.order_feedback.record(sequence, blocker);
        }
    }
    if client.village.attack_move || client.village.patrol_move {
        super::waypoints::ping(client, destination);
    }
    if !queued {
        cancel_move_mode(client);
    }
    sent
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

/// One message per chunk of at most MAX_FORMATION_MEMBERS units. A single
/// unit still uses the list message, so the server has one path per order.
fn send_lists(
    client: &mut Client,
    mut ids: Vec<aoe_core::EntityId>,
    message: impl Fn(u64, Vec<aoe_core::EntityId>) -> Message,
) -> bool {
    ids.sort_unstable();
    ids.dedup();
    let mut sent = false;
    for chunk in ids.chunks(aoe_core::MAX_FORMATION_MEMBERS) {
        let sequence = client.next_sequence;
        sent |= send(client, message(sequence, chunk.to_vec()));
    }
    sent
}

fn send_attack(
    client: &mut Client,
    mut entity_ids: Vec<aoe_core::EntityId>,
    target_id: aoe_core::EntityId,
) {
    entity_ids.sort_unstable();
    entity_ids.dedup();
    for ids in entity_ids.chunks(aoe_core::MAX_FORMATION_MEMBERS) {
        let sequence = client.next_sequence;
        let message = if let [entity_id] = ids {
            Message::AttackOrder {
                sequence,
                entity_id: *entity_id,
                target_id,
            }
        } else {
            Message::AttackSelectionOrder {
                sequence,
                entity_ids: ids.to_vec(),
                target_id,
            }
        };
        send(client, message);
    }
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
    for unit_ids in military(client).chunks(aoe_core::MAX_FORMATION_MEMBERS) {
        if unit_ids.len() > 1 {
            send(
                client,
                Message::FormationOrder {
                    sequence: client.next_sequence,
                    unit_ids: unit_ids.to_vec(),
                    formation: form,
                    destination: None,
                    queued: false,
                    attack_move: false,
                },
            );
        }
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
