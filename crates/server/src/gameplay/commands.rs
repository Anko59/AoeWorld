use super::{
    GameplayService, MAX_ACK_HISTORY, MAX_PENDING_COMMANDS_GLOBAL,
    MAX_PENDING_COMMANDS_PER_CONNECTION, PendingAction, PendingCommand, total_pending,
};
use aoe_core::{EntityId, WorldPosition};
use aoe_protocol::{CommandResult, GameplayRole, GameplayServerMessage};
use std::time::Instant;

impl GameplayService {
    pub async fn queue_move(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        destination: WorldPosition,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Move {
                entity_id,
                destination,
            },
        )
        .await;
    }
    pub async fn queue_gather(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        resource_id: u64,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Gather {
                entity_id,
                resource_id,
            },
        )
        .await;
    }
    pub async fn queue_attack(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        target_id: EntityId,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Attack {
                entity_id,
                target_id,
            },
        )
        .await;
    }
    pub async fn queue_train(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
        kind: aoe_protocol::UnitKind,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Train { building_id, kind },
        )
        .await;
    }
    pub async fn queue_cancel_training(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::CancelTraining { building_id },
        )
        .await;
    }
    async fn queue_command(&self, session_id: u64, sequence: u64, action: PendingAction) {
        let now = Instant::now();
        let current_tick = self.world.read().await.tick();
        let mut sessions = self.sessions.lock().await;
        let global_pending = total_pending(&sessions);
        let Some(session) = sessions.get_mut(&session_id) else {
            return;
        };
        let mut result = None;
        if session.role != GameplayRole::Controller {
            result = Some(CommandResult::RejectedNotController);
        } else if session.last_sequence.is_some_and(|last| sequence <= last)
            || session.acknowledgements.contains(&sequence)
        {
            result = Some(CommandResult::RejectedSequence);
        } else {
            let elapsed = now
                .duration_since(session.last_command_refill)
                .as_secs_f64();
            session.command_tokens = (session.command_tokens + elapsed * 10.0).min(20.0);
            session.last_command_refill = now;
            if session.command_tokens < 1.0
                || session.pending.len() >= MAX_PENDING_COMMANDS_PER_CONNECTION
            {
                result = Some(if session.command_tokens < 1.0 {
                    CommandResult::RejectedRateLimited
                } else {
                    CommandResult::RejectedQueueFull
                });
            } else if global_pending >= MAX_PENDING_COMMANDS_GLOBAL {
                result = Some(CommandResult::RejectedQueueFull);
            } else {
                session.last_sequence = Some(sequence);
                session.command_tokens -= 1.0;
                session
                    .pending
                    .push_back(PendingCommand { sequence, action });
            }
        }
        if let Some(result) = result {
            let _ = session.sender.try_send(GameplayServerMessage::CommandAck {
                sequence,
                result,
                applied_tick: current_tick,
            });
            session.acknowledgements.push_back(sequence);
            while session.acknowledgements.len() > MAX_ACK_HISTORY {
                session.acknowledgements.pop_front();
            }
        }
    }
}

pub(super) fn apply(
    world: &mut aoe_simulation::GameWorld,
    player: aoe_core::PlayerId,
    action: PendingAction,
) -> CommandResult {
    let result = match action {
        PendingAction::Move {
            entity_id,
            destination,
        } => {
            let Some(unit) = world.unit(entity_id) else {
                return CommandResult::RejectedUnknownEntity;
            };
            if unit.player != player {
                return CommandResult::RejectedNotOwner;
            }
            if !world.config().valid_map_position(destination) {
                return CommandResult::RejectedInvalidDestination;
            }
            world
                .issue_move(entity_id, destination)
                .map(|_| ())
                .map_err(aoe_simulation::EconomyError::Movement)
        }
        PendingAction::Gather {
            entity_id,
            resource_id,
        } => world.issue_gather(player, entity_id, resource_id),
        PendingAction::Attack {
            entity_id,
            target_id,
        } => world.issue_attack(player, entity_id, target_id),
        PendingAction::CancelTraining { building_id } => {
            world.issue_cancel_training(player, building_id)
        }
        PendingAction::Train { building_id, kind } => {
            world.issue_train(player, building_id, super::economy::native_kind(kind))
        }
    };
    use aoe_simulation::{EconomyError as E, GameWorldError as W};
    match result {
        Ok(()) => CommandResult::Accepted,
        Err(E::UnknownEntity | E::Movement(W::UnknownEntity)) => {
            CommandResult::RejectedUnknownEntity
        }
        Err(E::InvalidTarget) => CommandResult::RejectedInvalidTarget,
        Err(E::InsufficientResources) => CommandResult::RejectedInsufficientResources,
        Err(E::NotOwner) => CommandResult::RejectedNotOwner,
        Err(E::WrongUnitKind) => CommandResult::RejectedWrongUnitKind,
        Err(E::InvalidResource) => CommandResult::RejectedInvalidResource,
        Err(E::InsufficientFood) => CommandResult::RejectedInsufficientFood,
        Err(E::PopulationCap) => CommandResult::RejectedPopulationCap,
        Err(E::TrainingQueueFull) => CommandResult::RejectedTrainingQueueFull,
        Err(E::Capacity) => CommandResult::RejectedQueueFull,
        Err(E::Movement(W::Unreachable)) => CommandResult::RejectedUnreachable,
        Err(E::Movement(W::PathBudgetExceeded)) => CommandResult::RejectedPathBudgetExceeded,
        Err(E::Movement(W::Environment(_))) => CommandResult::RejectedResourceUnavailable,
        Err(_) => CommandResult::RejectedInvalidDestination,
    }
}

#[path = "commands/tests.rs"]
#[cfg(test)]
mod tests;
