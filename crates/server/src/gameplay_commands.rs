use super::{
    GameplayService, MAX_ACK_HISTORY, MAX_PENDING_COMMANDS_GLOBAL,
    MAX_PENDING_COMMANDS_PER_CONNECTION, PendingCommand, total_pending,
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
                session.pending.push_back(PendingCommand {
                    sequence,
                    entity_id,
                    destination,
                });
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
