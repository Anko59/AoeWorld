use super::*;
use aoe_core::PlayerId;
use aoe_simulation::{EconomyError, GameWorld, TileVisibility, UnitKind};

pub(super) fn attack_selection(
    world: &mut GameWorld,
    player: PlayerId,
    ids: &[EntityId],
    target: EntityId,
) -> Result<(), EconomyError> {
    let visible = world.unit_visible_to(player, target)
        || world.animal(target).is_some_and(|animal| {
            animal.owner == Some(player)
                || world.visibility(player, animal.position.tile_floor()) == TileVisibility::Visible
        })
        || world.building_entity_visible_to(player, target);
    if !visible {
        return Err(EconomyError::InvalidTarget);
    }
    world.issue_player_attack(player, ids, target)
}

/// Retain the first refusal when every member fails. Partial successes apply
/// in selection order and, like Stop, acknowledge the player action once.
pub(super) fn apply_each(
    world: &mut GameWorld,
    player: PlayerId,
    actions: impl Iterator<Item = PendingAction>,
) -> CommandResult {
    let mut aggregate = None;
    for action in actions {
        let result = apply(world, player, action);
        if aggregate.is_none() || result == CommandResult::Accepted {
            aggregate = Some(result);
        }
    }
    aggregate.unwrap_or(CommandResult::RejectedUnknownEntity)
}

pub(super) fn move_selection(
    world: &mut GameWorld,
    player: PlayerId,
    order: SelectionMove,
) -> CommandResult {
    let SelectionMove {
        unit_ids,
        formation,
        destination,
        queued,
        attack_move,
    } = order;
    let (mut formed, mut scalar): (Vec<_>, Vec<_>) = unit_ids.into_iter().partition(|&id| {
        world.unit(id).is_some_and(|u| {
            u.player == player
                && u.hp > 0
                && !u.kind.is_naval()
                && !matches!(
                    u.kind,
                    UnitKind::Villager | UnitKind::Steward | UnitKind::Vassal
                )
                && (aoe_simulation::unit_combat(u.kind).attack > 0
                    || matches!(u.kind, UnitKind::General | UnitKind::Monk))
        })
    });
    let mut group = None;
    if formed.len() > 1 {
        group = Some(apply(
            world,
            player,
            PendingAction::Formation {
                unit_ids: formed,
                formation,
                destination: Some(destination),
                queued,
                attack_move,
            },
        ));
    } else {
        scalar.append(&mut formed);
    }
    if scalar.is_empty() {
        return group.unwrap_or(CommandResult::RejectedUnknownEntity);
    }
    let mut accepted = Vec::new();
    let mut result = CommandResult::RejectedUnknownEntity;
    for (i, entity_id) in scalar.into_iter().enumerate() {
        let unit = world.unit(entity_id);
        let verdict = apply(
            world,
            player,
            PendingAction::Move {
                entity_id,
                destination,
                queued: queued && unit.is_some(),
                attack_move: attack_move && unit.is_some(),
            },
        );
        if verdict == CommandResult::Accepted {
            accepted.push(entity_id);
        }
        if i == 0 || verdict == CommandResult::Accepted {
            result = verdict;
        }
    }
    if accepted.len() > 1 {
        world.share_selection_waypoint(&accepted, destination, queued, attack_move);
    }
    if result == CommandResult::Accepted {
        result
    } else {
        group.unwrap_or(result)
    }
}
