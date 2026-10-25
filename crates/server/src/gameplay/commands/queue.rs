//! Bounded command admission and asynchronous terrain readiness.
use super::*;

impl GameplayService {
    pub(super) async fn queue_command(
        &self,
        session_id: u64,
        sequence: u64,
        action: PendingAction,
    ) {
        let arena = crate::gameplay::arena::enabled();
        if arena {
            crate::gameplay::arena::note_order();
        }
        let now = Instant::now();
        let (current_tick, preparation) = {
            let world = self.world.read().await;
            let request = crate::gameplay::preparation::Request::new(&world, &action);
            (world.tick(), request)
        };
        let mut sessions = self.sessions.lock().await;
        let global_pending = total_pending(&sessions);
        // One budget scales with the number of hosted seats, so many AI
        // connections do not exhaust a limit sized for a single human.
        let global_limit = MAX_PENDING_COMMANDS_GLOBAL.max(
            sessions
                .len()
                .saturating_mul(MAX_PENDING_COMMANDS_PER_CONNECTION),
        );
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
            // A person clicks; an AI issues a decision's worth of orders at once.
            let (refill, burst, queue) = if session.ai {
                (AI_ORDERS_PER_SECOND, AI_ORDER_BURST, AI_PENDING_COMMANDS)
            } else {
                (10.0, 20.0, MAX_PENDING_COMMANDS_PER_CONNECTION)
            };
            session.command_tokens = (session.command_tokens + elapsed * refill).min(burst);
            session.last_command_refill = now;
            if !arena && session.command_tokens < 1.0 || session.pending.len() >= queue {
                result = Some(if !arena && session.command_tokens < 1.0 {
                    CommandResult::RejectedRateLimited
                } else {
                    CommandResult::RejectedQueueFull
                });
            } else if global_pending >= global_limit {
                result = Some(CommandResult::RejectedQueueFull);
            } else {
                let prepared = match preparation.filter(|request| request.owner == session.viewer) {
                    Some(request) => self.preparation.prepare(request.generator, &request.tiles),
                    None => Ok(Vec::new()),
                };
                match prepared {
                    Ok(prepared) => {
                        session.last_sequence = Some(sequence);
                        session.command_tokens -= 1.0;
                        session.pending.push_back(PendingCommand {
                            sequence,
                            action,
                            prepared,
                        });
                    }
                    Err(()) => result = Some(CommandResult::RejectedQueueFull),
                }
            }
        }
        if let Some(result) = result {
            // A rejection is a verdict too: sent at once when the queue has room
            // (and nothing waits ahead of it), otherwise held, never dropped.
            let ack = GameplayServerMessage::CommandAck {
                sequence,
                result,
                applied_tick: current_tick,
            };
            if session.pending_acks.is_empty() {
                if let Err(tokio::sync::mpsc::error::TrySendError::Full(ack)) =
                    session.sender.try_send(ack)
                {
                    session.pending_acks.push_back(ack);
                }
            } else {
                session.pending_acks.push_back(ack);
            }
            session.acknowledgements.push_back(sequence);
            while session.acknowledgements.len() > MAX_ACK_HISTORY {
                session.acknowledgements.pop_front();
            }
        }
    }
}
