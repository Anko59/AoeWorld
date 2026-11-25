use super::deliver::deliver_on;
use super::fanout::{self, Built, View};
use super::{GameplayService, PendingCommand, Session};
use aoe_core::PlayerId;
use aoe_protocol::GameplayServerMessage;
use std::collections::BTreeMap;
use tokio::sync::{OwnedMutexGuard, oneshot};

type Intake = Vec<(u64, PlayerId, PendingCommand)>;
type Acks = BTreeMap<u64, Vec<GameplayServerMessage>>;
type Resources = (Vec<super::resources::ResourceUpdate>, Vec<u64>);

/// The next tick's commands, taken by an overlapped delivery before it
/// started, so that tick need not wait for the session lock.
#[derive(Default)]
pub(super) struct Overlap(std::sync::Mutex<Option<oneshot::Receiver<Intake>>>);

impl GameplayService {
    pub async fn tick(&self) {
        self.step(false).await;
    }

    /// One scheduled tick. With `overlap` (the server is behind its clock)
    /// this tick's views are delivered while the next tick applies its
    /// orders and advances: delivery needs the sessions, not the world.
    pub async fn tick_overlapped(&self, overlap: bool) {
        self.step(overlap).await;
    }

    fn intake(&self, sessions: &mut BTreeMap<u64, Session>) -> Intake {
        let mut commands = Vec::new();
        // Session IDs and accepted sequence numbers are already ordered.
        // Drain each ready prefix so a command cannot overtake its own terrain work.
        for (&id, session) in sessions.iter_mut() {
            while session
                .pending
                .front()
                .is_some_and(super::QueuedCommand::ready)
            {
                let Some(command) = session.pending.pop_front() else {
                    break;
                };
                commands.push((id, session.viewer, command.into_pending()));
            }
        }
        commands
    }

    async fn step(&self, overlap: bool) {
        let started = std::time::Instant::now();
        let stashed = self
            .overlap
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        let pending = match stashed {
            Some(taken) => taken.await.unwrap_or_else(|_| {
                tracing::error!("overlapped delivery lost the next tick's commands");
                Vec::new()
            }),
            None => self.intake(&mut *self.sessions.lock().await),
        };
        // The seats send what their last decision made, after this tick's
        // commands are fixed: those orders enter the next tick.
        self.flush_ai_seats().await;
        self.probe.phase("sessions", started.elapsed());
        let started = std::time::Instant::now();
        let mut world = self.world.clone().write_owned().await;
        self.probe.phase("world_lock", started.elapsed());
        let started = std::time::Instant::now();
        let orders = pending.len();
        // The tick this one starts at, for the AI seats' beat.
        let barrier = world.tick().0;
        let seats = self.ai_seats.sessions();
        let seat_log = self.ai_seats.log_handle();
        let humans: Vec<_> = self
            .slots
            .iter()
            .filter(|s| s.human)
            .map(|s| s.player)
            .collect();
        let human_players = humans.clone();
        let views: Vec<_> = self
            .human_views
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .copied()
            .collect();
        let recorded = self.record_inputs(&pending);
        // Attack-position and construction validation can be CPU work. Keep
        // it off the async runtime so one order cannot starve socket I/O.
        let applied = tokio::task::spawn_blocking(move || {
            let mut acknowledgements = BTreeMap::<u64, Vec<GameplayServerMessage>>::new();
            world.set_human_views(&views);
            for (session_id, player, command) in pending {
                let in_process = seats.contains(&session_id);
                let issuer = if humans.contains(&player) {
                    aoe_simulation::OrderIssuer::Player
                } else if in_process {
                    aoe_simulation::OrderIssuer::RustAi
                } else {
                    aoe_simulation::OrderIssuer::PythonAi
                };
                let sequence = command.sequence;
                let order = (in_process && seat_log.is_some())
                    .then(|| serde_json::to_string(&command.action).unwrap_or_default());
                let result = world.with_order_issuer(issuer, |w| {
                    w.record_order_decisions(command.action.decision_count());
                    super::commands::apply(w, player, command.action)
                });
                let tick = world.tick();
                if let (Some(order), Some(log)) = (order, &seat_log) {
                    crate::gameplay_match::record_applied(
                        log,
                        crate::gameplay_match::AppliedOrder {
                            tick: tick.0,
                            player,
                            sequence,
                            result,
                            order,
                        },
                    );
                }
                acknowledgements.entry(session_id).or_default().push(
                    GameplayServerMessage::CommandAck {
                        sequence,
                        result,
                        applied_tick: tick,
                    },
                );
            }
            (world, acknowledgements)
        })
        .await;
        let Ok((world, acknowledgements)) = applied else {
            tracing::error!("gameplay order worker failed");
            return;
        };
        self.probe.phase("orders", started.elapsed());
        let advance_started = std::time::Instant::now();
        let advanced = self.advance_world(world).await;
        self.probe.record(advance_started.elapsed(), orders);
        let Some(mut world) = advanced else {
            tracing::error!("gameplay tick worker failed");
            return;
        };
        if let Some(commands) = recorded
            && let Err(error) = self.record_finished(world.tick().0 - 1, commands, &world)
        {
            tracing::error!(%error, "command recording failed");
            return;
        }
        if world.tick().0.is_multiple_of(ROUTE_LOG_TICKS) {
            log_route_scheduler(&world);
        }
        // AI sessions are viewed every `AI_VIEW_TICKS`: an AI decides about
        // 1.6 times a second. Its players keep their building changes until then.
        let ai_due = world.tick().0.is_multiple_of(AI_VIEW_TICKS);
        let changes = if ai_due {
            world.take_remembered_building_changes()
        } else {
            world.take_remembered_building_changes_of(|player| human_players.contains(&player))
        };
        // The simulation has stepped: from here the world is only read, so
        // commands and map work may queue for the write lock while the views
        // are built on worker threads.
        let world = world.downgrade();
        // Views are read once the previous delivery has settled each
        // session's baselines (it holds the lock until then).
        let started = std::time::Instant::now();
        let views = {
            let sessions = self.sessions.lock().await;
            sessions
                .iter()
                // A fog toggle still reaches every session on the next tick.
                .filter(|(_, session)| {
                    ai_due
                        || !session.ai
                        || session.resident_fog.as_ref().map(|fog| fog.disabled)
                            != Some(world.fog_disabled())
                })
                .filter_map(|(id, session)| view(*id, session))
                .collect::<Vec<_>>()
        };
        self.probe.phase("sessions", started.elapsed());
        self.collect_ai_seats(barrier).await;
        let fanout_started = std::time::Instant::now();
        let last = self.fanout_probe.take_last_frame();
        let built = tokio::task::spawn_blocking(move || {
            let built = fanout::build(&world, &views, &changes, last);
            built.map(|built| (world, built))
        })
        .await
        .ok()
        .flatten();
        let Some((world, built)) = built else {
            tracing::error!("gameplay fan-out worker failed");
            // The tick's building changes were taken and are lost: every
            // session rebuilds its building baseline.
            for session in self.sessions.lock().await.values_mut() {
                session.rebuild_buildings = true;
            }
            return;
        };
        self.probe.phase("fanout_wait", fanout_started.elapsed());
        self.fanout_probe
            .record_unit_frame(built.frame.visited(), built.frame.work());
        self.fanout_probe.keep_frame(built.frame.clone());
        let mut sessions = self.sessions.clone().lock_owned().await;
        if overlap {
            // Resources read the world this tick stepped to, so they are
            // built before the next tick may take the world, and sent after
            // the views as on an ordinary tick.
            let started = std::time::Instant::now();
            let resources = self.resource_updates(&world, &mut sessions);
            drop(world);
            self.probe.phase("resources", started.elapsed());
            let (send, taken) = oneshot::channel();
            *self
                .overlap
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(taken);
            let service = self.clone();
            tokio::spawn(async move {
                service
                    .finish(sessions, built, acknowledgements, Some((send, resources)))
                    .await;
            });
        } else {
            drop(world);
            self.finish(sessions, built, acknowledgements, None).await;
        }
        let started = std::time::Instant::now();
        self.prefetch_routes().await;
        self.probe.phase("prefetch", started.elapsed());
    }

    /// Delivers a tick's views and resources. An overlapped delivery first
    /// takes the next tick's commands, so that tick can start at once.
    async fn finish(
        &self,
        mut sessions: OwnedMutexGuard<BTreeMap<u64, Session>>,
        built: Built,
        acknowledgements: Acks,
        next: Option<(oneshot::Sender<Intake>, Resources)>,
    ) {
        let overlapped = next.is_some();
        let mut resources = None;
        if let Some((next, built_resources)) = next {
            let _ = next.send(self.intake(&mut sessions));
            resources = Some(built_resources);
        }
        let delivery_started = std::time::Instant::now();
        let mut failed = deliver_on(&mut sessions, &built, acknowledgements, !overlapped);
        if let Some((ready, closed)) = resources {
            failed.extend(closed);
            failed.extend(super::resources::send_resources(&mut sessions, ready));
        }
        drop(sessions);
        for id in failed {
            self.disconnect(id).await;
        }
        let [viewers, frame, sessions] = built.phases;
        self.probe.phase("viewers", viewers);
        self.probe.phase("frame", frame);
        self.probe.phase("session_views", sessions);
        self.probe.phase("delivery", delivery_started.elapsed());
        self.fanout_probe
            .record([viewers, frame, sessions, delivery_started.elapsed()]);
        if !overlapped {
            let started = std::time::Instant::now();
            self.publish_resources().await;
            self.probe.phase("resources", started.elapsed());
        }
    }
}

fn view(session_id: u64, session: &Session) -> Option<View> {
    session.subscription.map(|subscription| View {
        session_id,
        subscription,
        vision: session.vision_revision,
        fog_disabled: session.resident_fog.as_ref().map(|fog| fog.disabled),
        viewer: session.viewer,
        primary: session.primary_unit_id,
        ai: session.ai,
        rebuild: session.rebuild_buildings,
    })
}

#[path = "tick/tests/dirty.rs"]
#[cfg(test)]
mod dirty;
#[path = "tick/tests/rebuild.rs"]
#[cfg(test)]
mod rebuild;
#[path = "tick/tests/scenario.rs"]
#[cfg(test)]
mod scenario;
#[path = "tick/tests.rs"]
#[cfg(test)]
mod tests;

/// Ticks between two views of an AI session; divides the AI fog and
/// heartbeat periods, so those still land on a viewed tick.
pub(super) const AI_VIEW_TICKS: u64 = 2;

/// Route-scheduler occupancy is logged once a minute at 20 Hz.
const ROUTE_LOG_TICKS: u64 = 1_200;

/// A stalled player shows up in the server log: units that wait for a route,
/// the slots held, and any drift between the counters and a recount.
fn log_route_scheduler(world: &aoe_simulation::GameWorld) {
    let stats = world.route_scheduler_stats();
    tracing::info!(
        tick = world.tick().0,
        planners = stats.planners,
        searches = stats.searches,
        waiting = stats.waiting_units,
        waiting_without_planner = stats.waiting_without_planner,
        waiting_players = stats.waiting_players,
        most_waiting = stats.most_waiting,
        most_waiting_player = ?stats.most_waiting_player.map(|player| player.0),
        oldest_wait_ticks = stats.oldest_wait_ticks,
        players_at_planner_cap = stats.players_at_planner_cap,
        players_at_search_cap = stats.players_at_search_cap,
        world_capacity_exhausted = stats.world_capacity_exhausted,
        orphan_planners = stats.orphan_planners,
        oldest_waiting = stats.oldest_waiting.as_deref().unwrap_or("none"),
        cache_entries = world.navigation_cache_usage().entries,
        units = world.units().count(),
        metrics = %stats.metrics,
        attribution = %crate::gameplay_match::routes::attribution_json(&stats.metrics.attribution),
        "route scheduler"
    );
    if let Some(error) = world.route_accounting_error() {
        tracing::warn!(%error, "route scheduler");
    }
}
