//! Bounded command admission and asynchronous terrain readiness.
use super::*;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
enum SelectionOrder {
    AutoScout { enabled: bool },
}

enum Work {
    Single(PendingAction),
    Selection {
        order: SelectionOrder,
        units: std::vec::IntoIter<EntityId>,
        result: Arc<Mutex<Option<CommandResult>>>,
    },
}

pub(in crate::gameplay) struct QueuedCommand {
    sequence: u64,
    work: Work,
    pub(in crate::gameplay) prepared: Vec<super::super::preparation::Ready>,
}

pub(in crate::gameplay) struct PendingCommand {
    pub(in crate::gameplay) sequence: u64,
    pub(in crate::gameplay) action: PendingAction,
    selection: Option<SelectionAck>,
    _prepared: Vec<super::super::preparation::Ready>,
}

struct SelectionAck {
    result: Arc<Mutex<Option<CommandResult>>>,
    last: bool,
}

impl QueuedCommand {
    pub(in crate::gameplay) fn ready(&self) -> bool {
        self.prepared
            .iter()
            .all(|flag| flag.load(std::sync::atomic::Ordering::Acquire))
    }

    /// Append scalar actions so each unit counts toward the existing budget.
    /// Return the unconsumed selection to leave at the head of its queue.
    pub(in crate::gameplay) fn take(
        self,
        session: u64,
        budget: usize,
        output: &mut Vec<(u64, PendingCommand)>,
    ) -> Option<Self> {
        match self.work {
            Work::Single(action) => {
                output.push((
                    session,
                    PendingCommand {
                        sequence: self.sequence,
                        action,
                        selection: None,
                        _prepared: self.prepared,
                    },
                ));
                None
            }
            Work::Selection {
                order,
                mut units,
                result,
            } => {
                let count = units.len().min(budget);
                let last = count == units.len();
                for (index, entity_id) in units.by_ref().take(count).enumerate() {
                    let action = match order {
                        SelectionOrder::AutoScout { enabled } => {
                            PendingAction::AutoScout { entity_id, enabled }
                        }
                    };
                    output.push((
                        session,
                        PendingCommand {
                            sequence: self.sequence,
                            action,
                            selection: Some(SelectionAck {
                                result: Arc::clone(&result),
                                last: last && index + 1 == count,
                            }),
                            _prepared: Vec::new(),
                        },
                    ));
                }
                (!last).then_some(Self {
                    sequence: self.sequence,
                    work: Work::Selection {
                        order,
                        units,
                        result,
                    },
                    prepared: self.prepared,
                })
            }
        }
    }
}

impl PendingCommand {
    pub(in crate::gameplay) fn apply(
        self,
        world: &mut aoe_simulation::GameWorld,
        player: aoe_core::PlayerId,
    ) -> Option<CommandResult> {
        let result = super::apply(world, player, self.action);
        let Some(selection) = self.selection else {
            return Some(result);
        };
        let mut aggregate = selection
            .result
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if aggregate.is_none() || result == CommandResult::Accepted {
            *aggregate = Some(result);
        }
        selection.last.then_some(*aggregate).flatten()
    }
}

impl GameplayService {
    pub(super) async fn queue_command(
        &self,
        session_id: u64,
        sequence: u64,
        action: PendingAction,
    ) {
        self.queue_work(session_id, sequence, Work::Single(action))
            .await;
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
        units: Vec<EntityId>,
        enabled: bool,
    ) {
        self.queue_selection(
            session_id,
            sequence,
            units,
            SelectionOrder::AutoScout { enabled },
        )
        .await;
    }

    async fn queue_selection(
        &self,
        session_id: u64,
        sequence: u64,
        units: Vec<EntityId>,
        order: SelectionOrder,
    ) {
        self.queue_work(
            session_id,
            sequence,
            Work::Selection {
                order,
                units: units.into_iter(),
                result: Arc::default(),
            },
        )
        .await;
    }

    async fn queue_work(&self, session_id: u64, sequence: u64, work: Work) {
        let arena = crate::gameplay::arena::enabled();
        if arena {
            crate::gameplay::arena::note_order();
        }
        let now = Instant::now();
        let (current_tick, preparation) = {
            let world = self.world.read().await;
            let request = match &work {
                Work::Single(action) => crate::gameplay::preparation::Request::new(&world, action),
                Work::Selection { .. } => None,
            };
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
        // One budget scales with the number of hosted seats, so many AI
        // connections do not exhaust a limit sized for a single human.
        let global_limit = MAX_PENDING_COMMANDS_GLOBAL.max(
            self.slots
                .len()
                .saturating_mul(MAX_PENDING_COMMANDS_PER_CONNECTION),
        );
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
                    10.0,
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
            } else if class_pending >= global_limit {
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
                            work,
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
