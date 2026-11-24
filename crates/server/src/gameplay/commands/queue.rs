//! Bounded command admission and asynchronous terrain readiness.
use super::*;
pub(in crate::gameplay) struct QueuedCommand {
    sequence: u64,
    action: PendingAction,
    pub(in crate::gameplay) prepared: Vec<super::super::preparation::Ready>,
}

pub(in crate::gameplay) struct PendingCommand {
    pub(in crate::gameplay) sequence: u64,
    pub(in crate::gameplay) action: PendingAction,
    _prepared: Vec<super::super::preparation::Ready>,
}

impl QueuedCommand {
    pub(in crate::gameplay) fn ready(&self) -> bool {
        self.prepared
            .iter()
            .all(|flag| flag.load(std::sync::atomic::Ordering::Acquire))
    }

    pub(in crate::gameplay) fn into_pending(self) -> PendingCommand {
        PendingCommand {
            sequence: self.sequence,
            action: self.action,
            _prepared: self.prepared,
        }
    }
}

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
        // Humans and AI seats are counted apart: a hundred AI seats filling the
        // shared budget must not refuse a person's build order.
        let pending_of = |ai: bool| {
            sessions
                .values()
                .filter(|s| s.ai == ai)
                .map(|s| s.pending.len())
                .sum::<usize>()
        };
        let class_pending = pending_of(sessions.get(&session_id).is_some_and(|s| s.ai));
        let Some(session) = sessions.get_mut(&session_id) else {
            return;
        };
        let mut result = None;
        if session.last_sequence.is_some_and(|last| sequence <= last)
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
                (
                    HUMAN_ORDERS_PER_SECOND,
                    aoe_protocol::HUMAN_COMMAND_BURST as f64,
                    MAX_PENDING_COMMANDS_PER_CONNECTION,
                )
            };
            session.command_tokens = (session.command_tokens + elapsed * refill).min(burst);
            session.last_command_refill = now;
            if !arena && session.command_tokens < 1.0 || session.pending.len() >= queue {
                result = Some(if !arena && session.command_tokens < 1.0 {
                    CommandResult::RejectedRateLimited
                } else {
                    CommandResult::RejectedQueueFull
                });
            } else if class_pending >= MAX_PENDING_COMMANDS_GLOBAL {
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
                        session.pending.push_back(QueuedCommand {
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

    pub(crate) async fn queue_stop_selection(
        &self,
        session_id: u64,
        sequence: u64,
        units: Vec<EntityId>,
    ) {
        self.queue_command(session_id, sequence, PendingAction::StopSelection(units))
            .await;
    }

    pub(crate) async fn queue_auto_scout_selection(
        &self,
        session_id: u64,
        sequence: u64,
        unit_ids: Vec<EntityId>,
        enabled: bool,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::AutoScoutSelection { unit_ids, enabled },
        )
        .await;
    }
}

impl PendingAction {
    pub(in crate::gameplay) fn decision_count(&self) -> u64 {
        match self {
            Self::AutoScoutSelection { unit_ids, .. } => unit_ids.len() as u64,
            _ => 1,
        }
    }
}
